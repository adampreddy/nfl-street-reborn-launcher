import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync,mkdirSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const html=`<!doctype html><html><head><style>body{margin:0}main{margin-left:180px;height:650px;overflow:auto}button{display:block;width:180px;height:42px;margin:12px}nav{position:absolute;left:0;width:170px}dialog button{display:block}</style></head><body><nav class="shell-nav"><a href="index.html" class="current">Play</a></nav><main id="main"><section id="screen"><button id="first" data-primary>Quick Game</button><button id="second">Game Mode</button><button id="third">Options</button></section></main><footer id="input-hints"></footer><script type="module" src="app-shell.js"></script></body></html>`;
const types={'.js':'text/javascript','.css':'text/css','.svg':'image/svg+xml','.png':'image/png','.jpg':'image/jpeg','.woff2':'font/woff2','.html':'text/html'};
const server=createServer((req,res)=>{try{const url=new URL(req.url,'http://localhost');
 if(url.pathname==='/'){res.setHeader('Content-Type','text/html');res.end(html);return;}
 const file=path.resolve(ui,'.'+decodeURIComponent(url.pathname));if(!file.startsWith(ui+path.sep))throw Error('outside');
 res.setHeader('Content-Type',types[path.extname(file)]||'application/octet-stream');res.end(readFileSync(file));
}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const browser=await chromium.launch({channel:'msedge',headless:true});
try{
 const page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
  window.pads=[{id:'Test pad',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:18},()=>({pressed:false,value:0}))}];
  Object.defineProperty(navigator,'getGamepads',{value:()=>pads});
  document.hasFocus=()=>true;
  window.frames=[];window.clock=0;window.requestAnimationFrame=fn=>(frames.push(fn),frames.length);window.cancelAnimationFrame=()=>{};
  window.tick=()=>{clock+=16;const pending=frames.splice(0);pending.forEach(fn=>fn(clock));};
 });
 await page.goto(`http://127.0.0.1:${server.address().port}`);await page.waitForFunction(()=>frames.length>0);
 const frame=()=>page.evaluate(()=>tick());
 const press=async(i)=>{await page.evaluate(i=>{pads[0].buttons[i]={pressed:true,value:1};tick();},i);await page.evaluate(i=>{pads[0].buttons[i]={pressed:false,value:0};tick();},i);};
 const focus=()=>page.evaluate(()=>document.activeElement.id);
 await frame();await press(13);assert.equal(await focus(),'first','first press reveals primary without skipping');
 await press(13);assert.equal(await focus(),'second');await press(13);assert.equal(await focus(),'third');
 await press(12);assert.equal(await focus(),'second');
 await page.evaluate(()=>{const dialog=document.createElement('dialog');dialog.innerHTML='<button id="modal-one">One</button><button id="modal-two">Two</button>';document.body.append(dialog);dialog.showModal();});
 await page.evaluate(()=>document.querySelector('dialog[open]').focus());
 await press(13);assert.equal(await focus(),'modal-one','lost modal focus recovers inside dialog');
 await press(13);assert.equal(await focus(),'modal-two','D-pad reaches dialog controls outside main');
 await press(1);assert.equal(await page.locator('dialog[open]').count(),0);
 // Unknown USB hat and reversed-axis calibration, persisted then used after reload.
 await page.evaluate(()=>{pads[0].mapping='';pads[0].axes=[0,0,3];});
 await page.locator('#menu-pad-setup').waitFor({state:'visible'});await page.locator('#menu-pad-setup').click();await page.getByRole('button',{name:'Begin',exact:true}).click();
 const settle=()=>page.evaluate(()=>{for(let i=0;i<15;i++)tick();});
 await settle();
 for(const v of [-1,1/7,5/7,-3/7]){
  await page.evaluate(v=>{pads[0].axes[2]=v;tick();},v);await page.evaluate(()=>pads[0].axes[2]=3);await settle();
 }
 for(const [axis,v] of [[1,1],[1,-1],[0,-1],[0,1]]){
  await page.evaluate(([axis,v])=>{pads[0].axes[axis]=v;tick();},[axis,v]);await page.evaluate(axis=>pads[0].axes[axis]=0,axis);await settle();
 }
 for(const index of [2,1,4,5]){await press(index);await settle();}
 assert.equal(await page.locator('dialog[open]').count(),0,'setup completes');
 await page.waitForFunction(()=>!document.body.dataset.controllerTesting);await settle();
 assert.equal(await page.evaluate(()=>Object.keys(JSON.parse(localStorage.getItem('reborn:menu-pad:1:Test pad:18:3')).bindings).length),12);
 await page.evaluate(()=>{document.getElementById('first').focus();tick();pads[0].axes[1]=-1;tick();});
 assert.equal(await focus(),'second','measured inverted raw Y drives Down correctly');
 await page.reload();await page.waitForFunction(()=>frames.length>0);
 await page.evaluate(()=>{pads[0].mapping='';pads[0].axes=[0,0,3];document.getElementById('first').focus();tick();pads[0].axes[2]=1/7;tick();});
 assert.equal(await focus(),'second','saved raw D-pad layout survives reload');
 assert.deepEqual(errors,[]);
 mkdirSync(path.join(root,'target/browser-review'),{recursive:true});
 await page.screenshot({path:path.join(root,'target/browser-review/controller-navigation.png')});
 await page.goto(`http://127.0.0.1:${server.address().port}/index.html`);
 await page.waitForFunction(()=>frames.length>0);
 assert.equal(await page.locator('#menu-pad-setup').isVisible(),false,'standard controllers need no setup prompt on Home');
 await page.goto(`http://127.0.0.1:${server.address().port}/controls.html`);
 await page.waitForFunction(()=>frames.length>0);
 await page.evaluate(()=>{pads[0].mapping='';pads[0].axes=[0,0,3];});
 await page.locator('#menu-pad-setup').click();
 assert.equal(await page.getByRole('button',{name:'Skip this control',includeHidden:true}).isVisible(),false,'skip is hidden before calibration begins');
 for(const width of [1280,800]){await page.setViewportSize({width,height:800});await page.evaluate(()=>document.fonts.ready);
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
  await page.screenshot({path:path.join(root,`target/browser-review/launcher-controller-setup-${width}.png`)});}
 await page.getByRole('button',{name:'Cancel',exact:true}).click();
 assert.deepEqual(errors,[]);
 console.log('PASS: first-focus, adjacent D-pad movement, modal navigation, raw hat/stick calibration, saved raw navigation');
}finally{await browser.close();server.close();}
