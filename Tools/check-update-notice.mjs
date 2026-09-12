import {chromium} from 'playwright';
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const source=readFileSync(new URL('../crates/reborn-ui/ui/update-notice.js',import.meta.url),'utf8');
const browser=await chromium.launch({channel:'msedge',headless:true});
async function run({automatic=false,active=false,current=false}={}){
 const context=await browser.newContext();
 const state={automatic,active,check:{state:current?'current':'available',current:'1',latest:'2',notes:'New controllers\nSmoother navigation'},activity:{downloading:false,ready:null},content:{phase:'up_to_date'},calls:[]};
 await context.exposeFunction('native',async(cmd,args)=>{
  state.calls.push({cmd,args});
  if(cmd==='onboarding_progress')return {complete:true};
  if(cmd==='update_policy')return {automatic:state.automatic};
  if(cmd==='update_check')return state.check;
  if(cmd==='content_status'||cmd==='content_check')return state.content;
  if(cmd==='update_activity')return state.activity;
  if(cmd==='session_active')return state.active;
  if(cmd==='music_status')return {phase:'idle'};
  if(cmd==='update_progress')return 50;
  if(cmd==='update_download'){
   state.activity.downloading=true;
   return await new Promise(resolve=>{state.finish=()=>{const ready={path:'verified.exe',check:state.check};state.activity={downloading:false,ready};resolve(ready);};});
  }
  if(cmd==='update_apply')return await new Promise(()=>{});
  throw Error(cmd);
 });
 await context.addInitScript(()=>{
  window.__TAURI__={core:{invoke:(cmd,args)=>window.native(cmd,args)}};
  const realTimeout=window.setTimeout,now=Date.now;
  let offset=0,seq=0;const timers=new Map();
  Date.now=()=>now()+offset;
  window.setInterval=(fn,ms)=>{timers.set(++seq,{fn,ms,next:Date.now()+ms});return seq;};
  window.clearInterval=id=>timers.delete(id);
  document.hasFocus=()=>true;
  window.advance=async ms=>{offset+=ms;for(const [id,t] of [...timers])if(t.next<=Date.now()&&timers.has(id)){t.next=Date.now()+t.ms;await t.fn();}await new Promise(r=>realTimeout(r,80));};
 });
 const p=await context.newPage();
 await p.route('http://updates.test/**',r=>r.fulfill({contentType:'text/html',body:'<body class="page-home"><main id="main"><div id="screen"></div></main></body>'}));
 await p.goto('http://updates.test/index.html');
 await p.addScriptTag({content:source});
 await p.waitForTimeout(180);
 return {p,state,context};
}
try{
 let t=await run();
 await t.p.getByRole('dialog',{name:'Launcher update',exact:true}).waitFor();
 assert.equal(await t.p.locator('.update-changes li').count(),2,'all release notes appear');
 await t.p.getByRole('button',{name:'Remind me later'}).click();
 await t.p.evaluate(()=>advance(125000));
 assert.equal(t.state.calls.some(c=>c.cmd==='update_download'),false);
 assert.ok(t.state.calls.filter(c=>c.cmd==='update_check').length>=2,'frequent checks continue after postponing');
 assert.equal(await t.p.getByRole('dialog').count(),0);
 await t.context.close();
 t=await run({automatic:true,active:true});
 for(let n=0;n<16;n++)await t.p.evaluate(()=>advance(1000));
 await t.p.waitForFunction(()=>document.querySelector('.update-card .notice-headline')?.textContent==='Downloading launcher');
 assert.equal(t.state.calls.filter(c=>c.cmd==='update_download').length,1,'one native download');
 // A document change cannot lose the download, its warning state, or the ready installer.
 await t.p.goto('http://updates.test/controls.html');await t.p.addScriptTag({content:source});
 await t.p.waitForTimeout(180);assert.equal(t.state.activity.downloading,true);
 t.state.finish();await t.p.evaluate(()=>advance(1600));
 assert.equal(t.state.calls.some(c=>c.cmd==='update_apply'),false,'never install during a game');
 t.state.active=false;await t.p.evaluate(()=>advance(1600));await t.p.evaluate(()=>advance(1600));
 assert.equal(t.state.calls.filter(c=>c.cmd==='update_apply').length,1,'install once after game exit');
 await t.context.close();
 t=await run({current:true});assert.equal(await t.p.getByRole('dialog').count(),0);
 t.state.check={state:'available',current:'1',latest:'3',notes:'New release'};
 await t.p.evaluate(()=>advance(125000));await t.p.evaluate(()=>advance(1600));
 await t.p.getByRole('dialog',{name:'Launcher update',exact:true}).waitFor();
 assert.ok(t.state.calls.filter(c=>c.cmd==='update_check').length>=2);
 await t.context.close();
 console.log('PASS: changelog popup, postpone without downloads, automatic countdown, frequent rechecks, navigation-safe downloads, install only after play ends.');
}finally{await browser.close();}
