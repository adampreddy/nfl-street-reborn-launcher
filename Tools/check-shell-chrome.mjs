import assert from 'node:assert/strict';
import {chromium} from 'playwright';
import {readFileSync,existsSync} from 'node:fs';
const browser=await chromium.launch({channel:'msedge',headless:true});
const page=await browser.newPage();
const errors=[];page.on('pageerror',e=>errors.push(e.message));
await page.route('http://shell.test/**',route=>{
 const path='crates/reborn-ui/ui'+new URL(route.request().url()).pathname;
 if(!existsSync(path))return route.fulfill({status:404,body:''});
 const ext=path.split('.').pop();
 return route.fulfill({body:readFileSync(path),contentType:({html:'text/html',js:'text/javascript',css:'text/css',png:'image/png',woff2:'font/woff2'})[ext]||'application/octet-stream'});
});
await page.addInitScript(()=>{
 window.pads=[];Object.defineProperty(navigator,'getGamepads',{value:()=>window.pads});document.hasFocus=()=>true;
 sessionStorage.setItem('reborn:input','controller');
 window.__TAURI__={core:{invoke:async cmd=>cmd==='session_active'?false:cmd==='music_status'?{phase:'idle',image_ok:true,enabled:true,slots:[]}:null}};
});
// Measure at rest: #main's page-enter translate leaves sub-pixel offsets while it runs.
const settle=()=>page.evaluate(async()=>{await document.fonts.ready;await Promise.race([Promise.all(document.getAnimations().filter(a=>a.effect?.getComputedTiming().iterations!==Infinity).map(a=>a.finished.catch(()=>{}))),new Promise(r=>setTimeout(r,5000))]);});
const box=sel=>page.locator(sel).first().evaluate(n=>{const r=n.getBoundingClientRect();return {x:Math.round(r.x*2)/2,y:Math.round(r.y*2)/2,w:Math.round(r.width*2)/2,h:Math.round(r.height*2)/2};});
try {
 await page.goto('http://shell.test/music.html');
 await page.getByRole('button',{name:'Add songs…',exact:true}).waitFor();
 assert.equal(await page.locator('.app-header').isVisible(),false);
 assert.equal(await page.locator('.input-footer').isVisible(),false);
 assert.equal(await page.locator('.brand img').getAttribute('src'),'art/nsr-sidebar-display.png');
 await page.locator('.brand img').evaluate(img=>img.decode());
 // The themed shell (theme.css, body.themed): the sidebar is 184px (160px at the minimum window), the
 // content fills the rest of the window from the top, and the closed save lane takes no room.
 for(const [width,nav] of [[800,160],[1280,184]]){
  await page.setViewportSize({width,height:800});await settle();
  const m=await box('#main');
  assert.deepEqual([m.x,m.y,m.w,m.h],[nav,0,width-nav,800],`content row at ${width}px`);
  assert.equal((await box('#save-lane')).h,0,'no pending change, no save lane');
 }
 await page.locator('.skip-link').evaluate(n=>n.focus());
 assert.equal(await page.locator('.skip-link').evaluate(n=>n===document.activeElement),false);
 await page.evaluate(()=>document.activeElement.blur());
 await page.keyboard.press('Tab');
 await page.locator('.skip-link').focus();
 assert.ok((await page.locator('.skip-link').boundingBox()).y>=0,'Keyboard users can still use skip link');
 await page.getByRole('button',{name:'Add songs…',exact:true}).focus();
 await page.evaluate(()=>{window.pads=[{id:'DualSense Wireless Controller',connected:true,mapping:'standard',index:0,axes:[0,0,0,0],buttons:Array.from({length:17},()=>({pressed:false,value:0}))}];window.dispatchEvent(new Event('gamepadconnected'));});
 // The Tab above left controller mode; the first pad input (D-pad Down) returns to it and the prompts show.
 await page.waitForTimeout(100);
 await page.evaluate(()=>{pads[0].buttons[13]={pressed:true,value:1};});await page.waitForTimeout(120);
 await page.evaluate(()=>{pads[0].buttons[13]={pressed:false,value:0};});
 await page.locator('.input-footer').waitFor({state:'visible',timeout:3000});
 assert.match(await page.locator('#input-hints').innerText(),/✕/);
 assert.doesNotMatch(await page.locator('#input-hints').innerText(),/Tab|Enter/);
 // With a prompt showing, the hint bar is its own 34px row under the content, which gives it the room.
 await settle();
 const withHints=await box('#main'),foot=await box('.input-footer');
 assert.deepEqual([withHints.y,withHints.h],[0,766],'the content row makes room for the hint bar');
 assert.deepEqual([foot.y,foot.h],[766,34],'the hint bar sits under the content');
 await page.evaluate(()=>{window.pads=[];window.dispatchEvent(new Event('gamepaddisconnected'));});
 assert.equal(await page.locator('.input-footer').isVisible(),false);
 await settle();
 const after=await box('#main');
 assert.deepEqual([after.y,after.h],[0,800],'the content row takes the room back from the hint bar');
 await page.screenshot({path:'target/review/shell-chrome.png'});
 assert.deepEqual(errors,[]);
 console.log('PASS: header removed, logo loaded, controller connect/disconnect hints, keyboard skip link, controller focus, themed shell geometry.');
} finally {await browser.close();}
