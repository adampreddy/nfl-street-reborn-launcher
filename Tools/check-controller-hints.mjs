import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const html=`<!doctype html><html><head></head><body class="shell"><nav class="shell-nav"><a href="index.html">Play</a></nav><main id="main"><section id="screen"><button id="first">First</button><button id="second">Second</button></section></main><footer class="input-footer"><span id="input-hints"></span></footer><script type="module" src="app-shell.js"></script></body></html>`;
const types={'.js':'text/javascript','.css':'text/css','.html':'text/html'};
const server=createServer((req,res)=>{try{const u=new URL(req.url,'http://x');if(u.pathname==='/'){res.setHeader('Content-Type','text/html');res.end(html);return;}
 const f=path.resolve(ui,'.'+decodeURIComponent(u.pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
try{
 const page=await browser.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.addInitScript(()=>{
  window.pads=[{id:'Test pad',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:18},()=>({pressed:false,value:0}))}];
  Object.defineProperty(navigator,'getGamepads',{value:()=>pads});
  document.hasFocus=()=>true;
  window.__TAURI__={core:{invoke:async cmd=>cmd==='session_active'?false:null}};
  window.frames=[];window.clock=0;window.requestAnimationFrame=fn=>(frames.push(fn),frames.length);window.cancelAnimationFrame=()=>{};
  window.tick=()=>{clock+=16;frames.splice(0).forEach(fn=>fn(clock));};
 });
 await page.goto(`http://127.0.0.1:${server.address().port}`);await page.waitForFunction(()=>frames.length>0);
 const hints=()=>page.evaluate(()=>document.getElementById('input-hints').children.length);
 const gamepad=()=>page.evaluate(()=>document.body.classList.contains('gamepad-input'));
 await page.evaluate(()=>tick());
 assert.equal(await hints(),0,'pad connected, mouse user: no hints');
 await page.evaluate(()=>{pads[0].buttons[13]={pressed:true,value:1};tick();pads[0].buttons[13]={pressed:false,value:0};tick();});
 assert.ok(await hints()>0,'pad input: hints appear');assert.equal(await gamepad(),true);
 await page.mouse.move(200,200);await page.mouse.move(260,240);
 assert.equal(await hints(),0,'mouse move: hints go');assert.equal(await gamepad(),false);
 await page.evaluate(()=>{pads[0].buttons[13]={pressed:true,value:1};tick();pads[0].buttons[13]={pressed:false,value:0};tick();});
 await page.keyboard.press('a');
 assert.equal(await hints(),0,'any key: hints go');
 // A controller test started with the mouse must still show its way out
 await page.evaluate(()=>{document.body.dataset.controllerTesting='1';window.dispatchEvent(new Event('gamepadconnected'));tick();});
 assert.match(await page.locator('#input-hints').innerText(),/Finish test/,'mouse-started test shows the exit hint');
 await page.evaluate(()=>{delete document.body.dataset.controllerTesting;window.dispatchEvent(new Event('gamepadconnected'));tick();});
 assert.equal(await hints(),0,'test over, mouse user: hints go again');
 // Launch audit H5: the prompts name the button by what is printed on it. Standard mapping is positional, so on a
 // Switch pad the bottom button (0, Select) says B and the right one (1, Back) says A; PlayStation keeps its symbols.
 const promptsFor=async(id)=>{await page.evaluate((id)=>{pads[0].id=id;tick();tick();pads[0].buttons[13]={pressed:true,value:1};tick();pads[0].buttons[13]={pressed:false,value:0};tick();},id);return page.locator('#input-hints').innerText();};
 assert.match(await promptsFor('Xbox Wireless Controller (STANDARD GAMEPAD Vendor: 045e Product: 0b13)'),/A\s+Select[\s\S]*B\s+Back[\s\S]*LB \/ RB/,'Xbox prompts');
 assert.match(await promptsFor('Pro Controller (STANDARD GAMEPAD Vendor: 057e Product: 2009)'),/B\s+Select[\s\S]*A\s+Back[\s\S]*L \/ R\s+Pages/,'Switch Pro prompts');
 assert.match(await promptsFor('DualSense Wireless Controller (STANDARD GAMEPAD Vendor: 054c Product: 0ce6)'),/✕\s+Select[\s\S]*○\s+Back/,'PlayStation prompts');
 await page.mouse.move(210,210);await page.mouse.move(250,250);
 // Review Focus 1: an unmapped pad must still say how to set it up
 await page.evaluate(()=>{pads[0].mapping='';pads[0].id='Unknown USB pad';window.dispatchEvent(new Event('gamepadconnected'));tick();});
 assert.match(await page.locator('#input-hints').innerText(),/Set up launcher controls/,'unmapped pad still offered setup');
 assert.deepEqual(errors,[]);
 console.log('check-controller-hints: PASS');
}finally{await browser.close();server.close();}
