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
 window.pads=[];Object.defineProperty(navigator,'getGamepads',{value:()=>window.pads});
 sessionStorage.setItem('reborn:input','controller');
 window.__TAURI__={core:{invoke:async cmd=>cmd==='session_active'?false:cmd==='music_status'?{phase:'idle',image_ok:true,enabled:true,slots:[]}:null}};
});
try {
 await page.goto('http://shell.test/music.html');
 await page.getByRole('button',{name:'Add song',exact:true}).waitFor();
 assert.equal(await page.locator('.app-header').isVisible(),false);
 assert.equal(await page.locator('.input-footer').isVisible(),false);
 assert.equal(await page.locator('.brand img').getAttribute('src'),'art/nsr-sidebar.png');
 await page.locator('.brand img').evaluate(img=>img.decode());
 for(const width of [800,1280]){
  await page.setViewportSize({width,height:800});
  assert.equal((await page.locator('main').boundingBox()).y,0);
  assert.equal(Math.round((await page.locator('main').boundingBox()).height),800);
 }
 await page.locator('.skip-link').evaluate(n=>n.focus());
 assert.equal(await page.locator('.skip-link').evaluate(n=>n===document.activeElement),false);
 await page.evaluate(()=>document.activeElement.blur());
 await page.keyboard.press('Tab');
 await page.locator('.skip-link').focus();
 assert.ok((await page.locator('.skip-link').boundingBox()).y>=0,'Keyboard users can still use skip link');
 await page.getByRole('button',{name:'Add song',exact:true}).focus();
 await page.evaluate(()=>{window.pads=[{id:'DualSense Wireless Controller',connected:true,mapping:'standard',index:0,axes:[0,0,0,0],buttons:Array.from({length:17},()=>({pressed:false,value:0}))}];window.dispatchEvent(new Event('gamepadconnected'));});
 assert.equal(await page.locator('.input-footer').isVisible(),true);
 assert.match(await page.locator('#input-hints').innerText(),/✕/);
 assert.doesNotMatch(await page.locator('#input-hints').innerText(),/Tab|Enter/);
 await page.evaluate(()=>{document.body.classList.add('page-setup','gamepad-input');});
 assert.equal((await page.locator('main').boundingBox()).y,68);
 await page.evaluate(()=>{document.body.classList.remove('page-setup');window.pads=[];window.dispatchEvent(new Event('gamepaddisconnected'));});
 assert.equal(await page.locator('.input-footer').isVisible(),false);
 assert.equal((await page.locator('main').boundingBox()).y,0);
 await page.screenshot({path:'target/review/shell-chrome.png'});
 assert.deepEqual(errors,[]);
 console.log('PASS: header removed, logo loaded, controller connect/disconnect hints, keyboard skip link, controller focus, responsive layout.');
} finally {await browser.close();}
