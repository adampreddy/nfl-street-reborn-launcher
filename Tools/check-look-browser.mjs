import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const types={'.js':'text/javascript','.css':'text/css','.svg':'image/svg+xml','.png':'image/png','.jpg':'image/jpeg','.woff2':'font/woff2','.html':'text/html'};
const server=createServer((req,res)=>{try{const u=new URL(req.url,'http://x');const f=path.resolve(ui,'.'+decodeURIComponent(u.pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const base=`http://127.0.0.1:${server.address().port}`;
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
try{
 const ctx=await browser.newContext({viewport:{width:1280,height:800}});
 await ctx.addInitScript(()=>{
  window.saved=JSON.parse(sessionStorage.getItem('stub:saved')||'null')||{windowGlass:true,slowAnimations:true};window.calls=[];
  const view=(glass,slow)=>({windowGlass:saved.windowGlass,slowAnimations:slow,glassSupported:true,glassApplied:glass,glassNote:null});
  window.__TAURI__={core:{invoke:async(cmd,args={})=>{calls.push({cmd,args});
   if(cmd==='launcher_look')return view(saved.windowGlass,saved.slowAnimations);
   if(cmd==='window_glass_apply')return view(args.enabled,saved.slowAnimations);
   if(cmd==='launcher_look_set'){if(args.windowGlass!==null&&args.windowGlass!==undefined)saved.windowGlass=args.windowGlass;if(args.slowAnimations!==null&&args.slowAnimations!==undefined)saved.slowAnimations=args.slowAnimations;sessionStorage.setItem('stub:saved',JSON.stringify(saved));return view(saved.windowGlass,saved.slowAnimations);}
   return null;}}};
 });
 const page=await ctx.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(base+'/settings.html#display');
 await page.waitForFunction(()=>document.documentElement.dataset.glass==='on');
const settled=()=>page.waitForFunction(()=>{const m=document.querySelector('.nav-marker');return m&&!m.classList.contains('still');});
 await settled();
 const dur=()=>page.evaluate(()=>getComputedStyle(document.querySelector('.nav-marker')).transitionDuration);
 assert.equal(await dur(),'0.96s','slow is the default');
 // Glass: preview off, then discard restores it and hides the bar
 await page.getByRole('switch',{name:'Window glass'}).click();
 assert.equal(await page.evaluate(()=>document.documentElement.dataset.glass),'off','glass previews off');
 await page.locator('.save-lane.open').waitFor();
 assert.match(await page.locator('.save-bar').innerText(),/1 unsaved change[\s\S]*Window glass/);
 // The save lane keeps its own grid row under #main, on screen, in mouse and in controller mode (the shell grid must not swallow it).
 const laneGeometry=async(mode)=>{
  const g=await page.evaluate(async()=>{await Promise.race([Promise.all(document.getAnimations().filter(a=>a.effect?.getComputedTiming().iterations!==Infinity).map(a=>a.finished.catch(()=>{}))),new Promise(r=>setTimeout(r,5000))]); /* infinite ones skipped, 5 s cap */
   const r=n=>{const b=document.querySelector(n).getBoundingClientRect();return {x:b.x,y:b.y,w:b.width,h:b.height};};
   return {lane:r('#save-lane'),main:r('#main'),nav:r('.shell-nav'),vw:innerWidth,vh:innerHeight};});
  assert.ok(g.lane.w>0&&g.lane.h>0,`${mode}: save lane has a size ${JSON.stringify(g.lane)}`);
  assert.ok(g.lane.x>=0&&g.lane.x+g.lane.w<=g.vw&&g.lane.y>=0&&g.lane.y+g.lane.h<=g.vh,`${mode}: save lane inside the window ${JSON.stringify(g.lane)}`);
  assert.ok(g.lane.y>=g.main.y+g.main.h-0.5,`${mode}: save lane below #main ${JSON.stringify(g)}`);
  assert.deepEqual([g.main.x,g.main.w],[g.nav.w,g.vw-g.nav.w],`${mode}: #main keeps the full content column`);
  assert.deepEqual([g.lane.x,g.lane.w],[g.nav.w,g.vw-g.nav.w],`${mode}: the lane is in the content column`);
 };
 await laneGeometry('mouse');
 await page.evaluate(()=>document.body.classList.add('gamepad-input'));
 await laneGeometry('controller');
 await page.evaluate(()=>document.body.classList.remove('gamepad-input'));
 await page.getByRole('button',{name:'Discard'}).click();
 assert.equal(await page.evaluate(()=>document.documentElement.dataset.glass),'on','discard restores glass');
 await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
 // Motion: switch off and save -> quick durations, persisted
 await page.getByRole('switch',{name:'Slow animations'}).click();
 assert.equal(await page.evaluate(()=>document.documentElement.dataset.motion),'quick','motion previews quick');
 await page.getByRole('button',{name:'Save changes'}).click();
 await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
 assert.equal(await page.evaluate(()=>saved.slowAnimations),false,'saved');
 assert.equal(await page.evaluate(()=>JSON.parse(localStorage.getItem('reborn:look')).slow),false,'cached for first paint');
 await page.reload();await page.waitForFunction(()=>document.documentElement.dataset.motion==='quick');await settled();
 assert.equal(await dur(),'0.24s','quick survives reload');
 // Reduced motion wins
 await page.emulateMedia({reducedMotion:'reduce'});
 assert.equal(await dur(),'0s','reduced motion');
 // A broken cache never breaks the page (Review Focus 4)
 await page.evaluate(()=>localStorage.setItem('reborn:look','{nope'));
 await page.reload();await page.waitForFunction(()=>document.documentElement.dataset.glass==='on');

 // Fix round 1: honest failure states
 const variant=async(mode)=>{
  const c=await browser.newContext({viewport:{width:1280,height:800}});
  await c.addInitScript((mode)=>{
   window.applies=[];window.savedGlass=mode==='preview-left'?true:true;
   const view=(glass)=>({windowGlass:true,slowAnimations:true,glassSupported:mode!=='unsupported',glassApplied:mode==='unsupported'||mode==='apply-failed'?false:glass,glassNote:mode==='unsupported'?'Needs Windows 11.':null});
   window.__TAURI__={core:{invoke:async(cmd,args={})=>{
    if(cmd==='launcher_look')return mode==='preview-left'?{...view(true),glassApplied:false}:view(true);
    if(cmd==='window_glass_apply'){applies.push(args.enabled);if(mode==='reject')throw 'boom';return view(args.enabled);}
    if(cmd==='launcher_look_set')return view(true);
    return null;}}};
  },mode);
  const pg=await c.newPage();pg.on('pageerror',e=>errors.push(e.message));
  await pg.goto(base+'/settings.html#display');await pg.getByRole('switch',{name:'Slow animations'}).waitFor();
  return pg;
 };
 { // (a) unsupported
  const pg=await variant('unsupported');
  assert.equal(await pg.getByRole('switch',{name:'Window glass'}).isDisabled(),true,'unsupported: disabled');
  assert.match(await pg.locator('.look-section').innerText(),/Needs Windows 11\./,'unsupported: note shown');
  assert.equal(await pg.evaluate(()=>document.documentElement.dataset.glass),'off');
  // (d) closed lane is inert
  assert.equal(await pg.evaluate(()=>document.getElementById('save-lane').inert),true,'closed lane inert');
 }
 { // (b) supported but the apply failed
  const pg=await variant('apply-failed');
  assert.equal(await pg.getByRole('switch',{name:'Window glass'}).isDisabled(),false,'failed: enabled');
  assert.equal(await pg.getByRole('switch',{name:'Window glass'}).getAttribute('aria-checked'),'true','failed: reflects draft');
  assert.match(await pg.locator('.look-section').innerText(),/The window glass could not be turned on\./);
 }
 { // (c) rejected preview: draft unchanged, no unhandled rejection, bar truthful, focus kept
  const pg=await variant('reject');
  await pg.getByRole('switch',{name:'Window glass'}).click();
  await pg.getByText('boom').waitFor().catch(()=>{});
  assert.equal(await pg.getByRole('switch',{name:'Window glass'}).getAttribute('aria-checked'),'true','rejected: draft unchanged');
  assert.equal(await pg.evaluate(()=>!!document.querySelector('.save-lane.open')),false,'rejected: no bar');
  assert.equal(await pg.evaluate(()=>document.activeElement?.id),'look-glass','focus kept');
  await pg.getByRole('switch',{name:'Slow animations'}).click();
  assert.equal(await pg.evaluate(()=>document.getElementById('save-lane').inert),false,'open lane not inert');
  assert.match(await pg.locator('.save-bar').innerText(),/1 unsaved change[\s\S]*Slow animations/,'bar lists only the real change');
 }
 { // preview left behind by another page is restored to the saved state, once
  const pg=await variant('preview-left');
  assert.deepEqual(await pg.evaluate(()=>applies),[true],'restored once');
 }
 { // launcher_look fails: the reason is shown, not "Reading..." forever
  const c=await browser.newContext({viewport:{width:1280,height:800}});
  await c.addInitScript(()=>{window.__TAURI__={core:{invoke:async(cmd)=>{if(cmd==='launcher_look')throw 'settings file is unreadable';return null;}}};});
  const pg=await c.newPage();pg.on('pageerror',e=>errors.push(e.message));
  await pg.goto(base+'/settings.html#display');
  await pg.getByText('Couldn’t read the launcher window settings:').waitFor({timeout:5000});
  const t=await pg.locator('#screen').innerText();
  assert.match(t,/Couldn’t read the launcher window settings: .*settings file is unreadable/,'failure reason shown');
  assert.doesNotMatch(t,/Reading the launcher window settings/,'not stuck on Reading');
 }
 assert.deepEqual(errors,[]);
 console.log('check-look-browser: PASS');
}finally{await browser.close();server.close();}
