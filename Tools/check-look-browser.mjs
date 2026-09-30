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
 assert.deepEqual(errors,[]);
 console.log('check-look-browser: PASS');
}finally{await browser.close();server.close();}
