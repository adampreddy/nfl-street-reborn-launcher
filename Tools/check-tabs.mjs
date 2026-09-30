// Segmented tabs (spec §3.4, §5.2): one highlight that slides to the selected option at --d2.
// The highlight moves with transform only (translateX + scaleX); width and left never transition.
//     node Tools/check-tabs.mjs
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const html=`<!doctype html><html lang="en" data-glass="off" data-motion="quick"><head><meta charset="utf-8"><link rel="stylesheet" href="theme.css"></head>
<body class="shell themed page-test"><main id="main"><section id="screen"></section></main>
<script type="module">
import {tabs} from './tabs.js';
let current='General';
window.draw=()=>document.getElementById('screen').replaceChildren(tabs('check',['General','Graphics','Files & storage'],current,n=>{current=n;window.draw();},{label:'Sections'}));
window.draw();window.ready=true;
</script></body></html>`;
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const u=new URL(req.url,'http://x');if(u.pathname==='/tabs.html'){res.setHeader('Content-Type','text/html');res.end(html);return;}
 const f=path.resolve(ui,'.'+decodeURIComponent(u.pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
try{
 const page=await browser.newPage({viewport:{width:1280,height:800}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{window.__TAURI__={core:{invoke:async()=>null}};});
 await page.goto(`http://127.0.0.1:${server.address().port}/tabs.html`);await page.waitForFunction(()=>window.ready);
 const gap=()=>page.evaluate(()=>{const i=document.querySelector('.tab-ind').getBoundingClientRect(),t=document.querySelector('.tab[aria-pressed="true"]').getBoundingClientRect();return Math.max(Math.abs(i.x-t.x),Math.abs(i.width-t.width),Math.abs(i.height-t.height));});
 await page.waitForTimeout(400);
 assert.ok(await gap()<1.5,"the highlight sits on the selected tab");
 assert.match(await page.locator('.tab-ind').evaluate(n=>getComputedStyle(n).transitionDuration),/^0\.18s/,'quick: --d2');
 const tp=await page.locator('.tab-ind').evaluate(n=>getComputedStyle(n).transitionProperty);
 assert.doesNotMatch(tp,/width|left|all/,`highlight transitions transform only, got: ${tp}`);
 assert.match(tp,/transform/,'highlight transitions transform');
 assert.equal(await page.locator('.tabs').evaluate(n=>getComputedStyle(n).borderTopLeftRadius),'14px','.tabs radius is --r-lg');
 await page.evaluate(()=>document.documentElement.dataset.motion='slow');
 const from=await page.locator('.tab[aria-pressed="true"]').evaluate(n=>n.getBoundingClientRect().x);
 await page.getByRole('button',{name:'Files & storage'}).click();
 await page.waitForTimeout(200);
 const mid=await page.locator('.tab-ind').evaluate(n=>n.getBoundingClientRect().x);
 const to=await page.locator('.tab[aria-pressed="true"]').evaluate(n=>n.getBoundingClientRect().x);
 assert.ok(mid>from+1&&mid<to-1,`it slides: mid-way x ${mid} lies between ${from} and ${to}`);
 await page.waitForTimeout(900);
 assert.ok(await gap()<1.5,'it settles on Files & storage');
 assert.deepEqual(await page.locator('.tab').evaluateAll(ns=>ns.map(n=>n.getAttribute('aria-pressed'))),['false','false','true']);
 assert.equal(await page.locator('.tab-ind').count(),1,'one highlight');
 // keyboard: arrows move focus between tabs (Enter/Space picks), Home/End jump
 await page.getByRole('button',{name:'General'}).focus();
 await page.keyboard.press('ArrowRight');
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),'Graphics','ArrowRight focuses the next tab');
 await page.keyboard.press('ArrowLeft');
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),'General','ArrowLeft focuses the previous tab');
 await page.keyboard.press('ArrowLeft');
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),'General','no wrap at the start');
 await page.keyboard.press('End');
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),'Files & storage','End focuses the last tab');
 await page.emulateMedia({reducedMotion:'reduce'});
 assert.match(await page.locator('.tab-ind').evaluate(n=>getComputedStyle(n).transitionDuration),/^0s/,'reduced motion: instant');
 assert.deepEqual(errors,[]);
 console.log('check-tabs: PASS');
}finally{await browser.close();server.close();}
