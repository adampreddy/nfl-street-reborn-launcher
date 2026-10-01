// The shell every page shares (spec §6) on a page that loads only theme.css (body.themed), and the
// same shell, in absolute terms, on a real themed page (online.html; P4-17).
//     node Tools/check-themed-shell.mjs
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const nav=readFileSync(path.join(ui,'index.html'),'utf8').match(/<nav class="shell-nav"[\s\S]*?<\/nav>/)[0];
const fixture=`<!doctype html><html lang="en" data-glass="off" data-motion="quick"><head><meta charset="utf-8"><link rel="stylesheet" href="theme.css"></head>
<body class="shell themed page-test"><a class="skip-link" href="#main">Skip to content</a>${nav}
<main id="main" tabindex="-1"><div class="page-heading"><span class="eyebrow">NFL STREET REBORN</span><h1>Title</h1><p class="purpose">Subtitle</p></div>
<section id="screen"><button class="btn" id="b1">One</button><button id="plain">Plain</button><p class="refusal">Nope</p></section></main>
<footer class="input-footer"><span id="input-hints"></span><span>NFL STREET REBORN</span></footer></body></html>`;
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const u=new URL(req.url,'http://x');if(u.pathname==='/fixture.html'){res.setHeader('Content-Type','text/html');res.end(fixture);return;}
 const f=path.resolve(ui,'.'+decodeURIComponent(u.pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const base=`http://127.0.0.1:${server.address().port}`;
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
const box=(p,sel)=>p.locator(sel).first().evaluate(n=>{const r=n.getBoundingClientRect();return {x:r.x,y:r.y,w:r.width,h:r.height};});
// Measure at rest: #main's page-enter translate leaves sub-pixel float error in boxes while it runs.
const settle=p=>p.evaluate(async()=>{await document.fonts.ready;await Promise.race([Promise.all(document.getAnimations().filter(a=>a.effect?.getComputedTiming().iterations!==Infinity).map(a=>a.finished.catch(()=>{}))),new Promise(r=>setTimeout(r,5000))]); /* infinite ones skipped, 5 s cap */});
const navBoxes=p=>p.locator('.shell-nav a').evaluateAll(ns=>ns.map(n=>{const r=n.getBoundingClientRect();return [r.x,r.y,r.width,r.height].map(v=>Math.round(v*2)/2).join(',');}));
try{
 const ctx=await browser.newContext({viewport:{width:1280,height:800}});
 const p=await ctx.newPage();const errors=[];p.on('pageerror',e=>errors.push(e.message));
 await p.goto(base+'/fixture.html');await settle(p);
 assert.equal((await box(p,'.shell-nav')).w,184,'sidebar is 184px');
 const items=await p.locator('.shell-nav a:not(.brand)').evaluateAll(ns=>ns.map(n=>n.getBoundingClientRect().height));
 assert.ok(items.length===8&&items.every(h=>h===40),'every nav item is 40px: '+items);
 assert.equal((await box(p,'.shell-nav .brand')).h,124,'logo slot as it is today');
 const main=await box(p,'#main');assert.deepEqual([main.x,main.w,main.h],[184,1096,800],'content fills the rest of the window');
 assert.equal(await p.locator('.input-footer').evaluate(n=>getComputedStyle(n).display),'none','no prompt, no hint bar');
 const skip=await box(p,'.skip-link');assert.ok(skip.y+skip.h<=0,'skip link off screen until focused');
 assert.equal(await p.locator('.page-heading h1').evaluate(n=>getComputedStyle(n).fontSize+' '+getComputedStyle(n).fontWeight),'30px 700');
 assert.equal(await p.locator('.page-heading .eyebrow').evaluate(n=>getComputedStyle(n).display),'none');
 assert.equal((await box(p,'#b1')).h,38,'.btn is 38px');
 assert.equal((await box(p,'#plain')).h,38,'a classless button (shared dialogs) looks like .btn');
 // A prompt appears: the bar is 34px and the content gives it the room.
 await p.evaluate(()=>{const s=document.createElement('span');s.className='input-hint';s.innerHTML='<kbd>A</kbd> Select';document.getElementById('input-hints').append(s);});
 assert.equal(await p.locator('.input-footer').evaluate(n=>getComputedStyle(n).display),'flex');
 assert.equal((await box(p,'.input-footer')).h,34);
 assert.equal((await box(p,'#main')).h,766);
 assert.equal(await p.locator('.input-footer > span:not(#input-hints)').evaluate(n=>getComputedStyle(n).display),'none','the hint bar shows prompts only');
 const themedNav=await navBoxes(p);
 // A real page on theme.css + its generated layout file (pages/online.css, spec §9). Until Task 10 this compared the
 // sidebar with Online still on the legacy sheets; every page is themed now, so the shell is checked in absolute terms
 // there instead (P4-17): sidebar width and items, the heading where the shell puts it, the main column, no hint bar.
 const real=await ctx.newPage();
 await real.addInitScript(()=>{window.__TAURI__={core:{invoke:async()=>null}};});
 const shellOf=async(pg,w)=>{
  await pg.goto(base+'/online.html');await pg.locator('.shell-nav a').first().waitFor();await settle(pg);
  assert.ok(await pg.evaluate(()=>document.body.classList.contains('themed')),'online.html is themed');
  const nav=await box(pg,'.shell-nav'),m=await box(pg,'#main'),h=await box(pg,'.page-heading h1'),sc=await box(pg,'#screen');
  return {nav,m,h,sc,footer:await pg.locator('.input-footer').evaluate(n=>getComputedStyle(n).display),w};
 };
 const s1=await shellOf(real,1280);
 assert.deepEqual([s1.nav.x,s1.nav.y,s1.nav.w,s1.nav.h],[0,0,184,800],'themed page: sidebar 184px, full height');
 assert.deepEqual(await navBoxes(real),themedNav,'themed page: the same sidebar items as the shell fixture');
 assert.deepEqual([s1.m.x,s1.m.y,s1.m.w,s1.m.h],[184,0,1096,800],'themed page: main column fills the rest');
 // The page's own padding places the heading (Online keeps its legacy 36px/32px); the shell's part is that it sits in
 // the main column, at the top, aligned with the content under it.
 const headingAt=(s)=>{assert.ok(s.h.w>0&&s.h.h>0,'themed page: heading shown');assert.equal(Math.round(s.h.x),Math.round(s.sc.x),'themed page: heading aligned with the content');
  assert.ok(s.h.x>=s.m.x+16&&s.h.y>=16&&s.h.y<=48&&s.h.y+s.h.h<=s.sc.y,`themed page: heading at the top of the main column (${JSON.stringify(s.h)})`);};
 headingAt(s1);
 assert.equal(s1.footer,'none','themed page: no prompt, no hint bar');
 await real.setViewportSize({width:800,height:560});
 const s2=await shellOf(real,800);
 assert.deepEqual([s2.nav.w,s2.m.x,s2.m.w,s2.m.h],[160,160,640,560],'themed page at 800x560: 160px sidebar, main fills the rest');
 assert.equal(s2.footer,'none');headingAt(s2);
 assert.ok(await real.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'themed page: no sideways scroll at 800x560');
 // Minimum window.
 await p.setViewportSize({width:800,height:560});await p.goto(base+'/fixture.html');await settle(p);
 assert.equal((await box(p,'.shell-nav')).w,160);
 assert.ok(await p.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no sideways scroll at 800x560');
 assert.deepEqual(errors,[]);
 console.log('check-themed-shell: PASS');
}finally{await browser.close();server.close();}
