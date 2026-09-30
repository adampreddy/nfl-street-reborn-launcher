// The menu (spec §3.4, §5.2): a glass list anchored to its button, a check on the current option,
// keyboard and pad inside it, and B / Esc / a click outside close it with focus back on the button.
//     node Tools/check-menu.mjs
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const html=`<!doctype html><html lang="en" data-glass="off" data-motion="quick"><head><meta charset="utf-8"><link rel="stylesheet" href="theme.css">
<style>#low{position:absolute;left:40px;bottom:40px}#high{position:absolute;right:40px;top:40px}</style></head>
<body class="shell themed page-test"><nav class="shell-nav"><a href="index.html">Play</a></nav>
<main id="main" tabindex="-1"><section id="screen"><button id="low" class="btn" aria-haspopup="menu">Graphics</button><button id="high" class="btn" aria-haspopup="menu">Display</button></section></main>
<footer class="input-footer"><span id="input-hints"></span></footer>
<script type="module">
import {openMenu} from './menu.js';
const options=[{value:'performance',label:'Performance'},{value:'balanced',label:'Balanced'},{value:'quality',label:'All-Out'}];
for(const id of ['low','high'])document.getElementById(id).addEventListener('click',e=>openMenu(e.currentTarget,{label:id,options,current:'balanced',onPick:v=>{window.picked=v;}}));
window.ready=true;
</script><script type="module" src="app-shell.js"></script></body></html>`;
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const u=new URL(req.url,'http://x');if(u.pathname==='/menu.html'){res.setHeader('Content-Type','text/html');res.end(html);return;}
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
 await page.goto(`http://127.0.0.1:${server.address().port}/menu.html`);
 await page.waitForFunction(()=>window.ready&&frames.length>0);
 const press=b=>page.evaluate(b=>{pads[0].buttons[b]={pressed:true,value:1};tick();pads[0].buttons[b]={pressed:false,value:0};tick();},b);
 const menu=page.locator('dialog.menu[open]');
 const closed=()=>page.waitForFunction(()=>!document.querySelector('dialog.menu'),null,{polling:50});
 const active=()=>page.evaluate(()=>document.activeElement.dataset.value||document.activeElement.id);
 await page.locator('#low').click();await menu.waitFor();
 assert.equal(await page.evaluate(()=>document.querySelector('dialog.menu').parentElement===document.body),true,'mounted on body');
 assert.equal(await page.getByRole('menuitemradio').count(),3);
 assert.equal(await page.getByRole('menuitemradio',{name:'Balanced'}).getAttribute('aria-checked'),'true','a check on the current option');
 assert.equal(await page.locator('#low').getAttribute('aria-expanded'),'true');
 assert.equal(await active(),'balanced','focus starts on the current option');
 const geo=await page.evaluate(()=>{const m=document.querySelector('dialog.menu'),r=m.getBoundingClientRect(),a=document.getElementById('low').getBoundingClientRect();return {mb:r.bottom,at:a.top,side:m.dataset.side};});
 assert.ok(geo.mb<=geo.at&&geo.side==='above','opens above its button: '+JSON.stringify(geo));
 assert.equal(await menu.evaluate(n=>getComputedStyle(n).animationDuration),'0.18s','quick motion: --d2');
 await page.keyboard.press('ArrowDown');
 assert.equal(await active(),'quality','arrow keys move inside');
 await page.keyboard.press('Enter');await closed();
 assert.equal(await page.evaluate(()=>window.picked),'quality');
 assert.equal(await active(),'low','focus returns to the button');
 assert.equal(await page.locator('#low').getAttribute('aria-expanded'),'false');
 await page.locator('#low').click();await menu.waitFor();await page.keyboard.press('Escape');await closed();
 assert.equal(await active(),'low','Esc closes');
 await page.locator('#low').click();await menu.waitFor();await page.mouse.click(640,200);await closed();
 assert.equal(await page.evaluate(()=>window.picked),'quality','a click outside picks nothing');
 await page.locator('#high').click();await menu.waitFor();
 assert.equal(await page.evaluate(()=>document.querySelector('dialog.menu').dataset.side),'below','no room above: opens below');
 await page.keyboard.press('Escape');await closed();
 await page.evaluate(()=>document.documentElement.dataset.motion='slow');
 await page.locator('#low').click();await menu.waitFor();
 assert.equal(await menu.evaluate(n=>getComputedStyle(n).animationDuration),'0.72s','slow motion: --d2');
 await page.keyboard.press('Escape');await closed();
 await page.locator('#low').click();await menu.waitFor();
 await page.evaluate(()=>tick());await press(13);
 assert.equal(await active(),'quality','D-pad moves inside the menu');
 await press(1);await closed();
 assert.equal(await active(),'low','B closes it and focus returns');
 await page.emulateMedia({reducedMotion:'reduce'});
 await page.locator('#low').click();await menu.waitFor();
 assert.equal(await menu.evaluate(n=>getComputedStyle(n).animationName),'none','reduced motion: no animation');
 assert.deepEqual(errors,[]);
 console.log('check-menu: PASS');
}finally{await browser.close();server.close();}
