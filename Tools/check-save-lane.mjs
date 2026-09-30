// The save lane (spec §7): its own row under #main; a failed save says why in the bar; a pad reaches
// the bar from the bottom of the page, moves along it, goes back up, and Back goes to the sidebar.
// While changes are pending, nothing drops them: LB/RB do nothing and a sidebar link leads to the bar.
//     node Tools/check-save-lane.mjs
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const nav=readFileSync(path.join(ui,'index.html'),'utf8').match(/<nav class="shell-nav"[\s\S]*?<\/nav>/)[0];
const html=`<!doctype html><html lang="en" data-glass="off" data-motion="quick"><head><meta charset="utf-8"><link rel="stylesheet" href="theme.css"></head>
<body class="shell themed page-test has-save-lane">${nav}
<main id="main" tabindex="-1"><section id="screen"><button class="btn" id="a">First</button><br><button class="btn" id="b">Second</button></section></main>
<div id="save-lane"></div><footer class="input-footer"><span id="input-hints"></span></footer>
<script type="module">
import {mountSaveBar} from './savebar.js';
window.outcome=null;window.saves=0;
window.bar=mountSaveBar(document.getElementById('save-lane'),{
 onSave:async()=>{if(window.outcome)throw new Error(window.outcome);window.saves++;window.bar.update([]);},
 onDiscard:async()=>{window.bar.update([]);},
 returnFocus:()=>document.getElementById('b')});
window.ready=true;
</script><script type="module" src="app-shell.js"></script></body></html>`;
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const u=new URL(req.url,'http://x');if(u.pathname==='/lane.html'){res.setHeader('Content-Type','text/html');res.end(html);return;}
 const f=path.resolve(ui,'.'+decodeURIComponent(u.pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
try{
 const page=await browser.newPage({viewport:{width:1280,height:800}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
  window.pads=[{id:'Test pad',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:18},()=>({pressed:false,value:0}))}];
  Object.defineProperty(navigator,'getGamepads',{value:()=>pads});
  document.hasFocus=()=>true;
  window.__TAURI__={core:{invoke:async cmd=>cmd==='session_active'?false:null}};
  window.frames=[];window.clock=0;window.requestAnimationFrame=fn=>(frames.push(fn),frames.length);window.cancelAnimationFrame=()=>{};
  window.tick=()=>{clock+=16;frames.splice(0).forEach(fn=>fn(clock));};
 });
 await page.goto(`http://127.0.0.1:${server.address().port}/lane.html`);
 await page.waitForFunction(()=>window.ready&&frames.length>0);
 await page.evaluate(()=>tick()); // one neutral frame arms the pad reader (menu-pad.js), as check-controller-navigation does
 const press=b=>page.evaluate(b=>{pads[0].buttons[b]={pressed:true,value:1};tick();pads[0].buttons[b]={pressed:false,value:0};tick();},b);
 const lane=()=>page.evaluate(()=>{const l=document.getElementById('save-lane');return {open:l.classList.contains('open'),inert:l.inert};});
 assert.deepEqual(await lane(),{open:false,inert:true},'closed until something changes');
 await page.evaluate(()=>bar.update(['Emulator location']));
 assert.deepEqual(await lane(),{open:true,inert:false});
 await page.waitForTimeout(400);
 const edge=await page.evaluate(()=>({main:document.getElementById('main').getBoundingClientRect().bottom,lane:document.getElementById('save-lane').getBoundingClientRect().top}));
 assert.ok(Math.abs(edge.main-edge.lane)<0.5,'the lane is its own row: the content ends where it starts');
 assert.match(await page.locator('.save-bar').innerText(),/1 unsaved change[\s\S]*Emulator location/);
 // A failed save says why, keeps the bar, and gives the buttons back.
 await page.evaluate(()=>window.outcome='The disk is full.');
 await page.getByRole('button',{name:'Save changes'}).click();
 await page.locator('.save-bar [role=alert]').waitFor();
 assert.equal(await page.locator('.save-bar [role=alert]').innerText(),'The disk is full.');
 assert.equal((await lane()).open,true,'the bar stays open');
 assert.equal(await page.getByRole('button',{name:'Save changes'}).isEnabled(),true);
 // The next attempt clears the old reason.
 await page.evaluate(()=>window.outcome=null);
 await page.getByRole('button',{name:'Save changes'}).click();
 await page.waitForFunction(()=>!document.getElementById('save-lane').classList.contains('open'));
 assert.equal(await page.evaluate(()=>saves),1);
 assert.equal(await page.locator('.save-bar [role=alert]').isHidden(),true);
 // Pad: Down from the last control enters the bar at Save; Left moves along it; Up goes back; Back goes to the sidebar.
 await page.evaluate(()=>bar.update(['Window glass']));
 await page.evaluate(()=>document.getElementById('b').focus());
 await press(13);
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),'Save changes','D-pad down reaches the save bar');
 await press(14);
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),'Discard','left moves along the bar');
 await press(12);
 assert.equal(await page.evaluate(()=>document.getElementById('main').contains(document.activeElement)),true,'up goes back into the page');
 await press(13);await press(1);
 assert.equal(await page.evaluate(()=>location.pathname),'/lane.html','Back from the bar never leaves the page');
 assert.equal(await page.evaluate(()=>!!document.activeElement.closest('.shell-nav')),true,'Back from the bar goes to the sidebar');
 // Pending changes are never dropped silently (P2-12): LB/RB do nothing while the bar is open...
 await press(5);
 assert.equal(await page.evaluate(()=>location.pathname),'/lane.html','RB with pending changes stays on the page');
 await press(4);
 assert.equal(await page.evaluate(()=>location.pathname),'/lane.html','LB with pending changes stays on the page');
 assert.equal((await lane()).open,true,'the changes are still pending after LB/RB');
 assert.equal(await page.evaluate(()=>!!document.activeElement.closest('.shell-nav')),true,'LB/RB with pending changes are a no-op: focus stays put');
 // ...and a sidebar link (mouse, or pad A) leads to the save bar instead of leaving.
 await page.locator('.shell-nav a[href="mods.html"]').click();
 await page.waitForTimeout(150);
 assert.equal(await page.evaluate(()=>location.pathname),'/lane.html','a sidebar click with pending changes stays on the page');
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),'Save changes','a sidebar click with pending changes moves focus to the save bar');
 await page.evaluate(()=>document.querySelector('.shell-nav a[href="music.html"]').focus());
 await press(0);
 await page.waitForTimeout(150);
 assert.equal(await page.evaluate(()=>location.pathname),'/lane.html','pad A on a sidebar link with pending changes stays on the page');
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),'Save changes','pad A on a sidebar link moves focus to the save bar');
 assert.equal((await lane()).open,true,'the changes are still pending after a sidebar click');
 assert.deepEqual(errors,[]);
 // With nothing pending, the sidebar navigates exactly as before.
 await page.evaluate(()=>bar.update([]));
 await Promise.all([page.waitForURL(/\/mods\.html$/),page.locator('.shell-nav a[href="mods.html"]').click()]);
 console.log('check-save-lane: PASS');
}finally{await browser.close();server.close();}
