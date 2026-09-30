// Pad moves keep to their column outside Settings (spec §6; P2-22). Settings asks for a "nearest row"
// tie-break (data-nav="rows"); every other page keeps plain column moves. Driven with the real app-shell
// pad reader, a stubbed getGamepads and a hand-cranked animation clock (as check-controller-navigation).
//     node Tools/check-pad-columns.mjs
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync} from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const source=readFileSync(path.join(root,'Tools/check-shell-pages.mjs'),'utf8');
const fixture=vm.runInNewContext(source.slice(source.indexOf('const FULL ='),source.indexOf('// Every field null'))+'\nFULL');
Object.assign(fixture,{session_active:false,ui_notices:[],update_activity:{downloading:false,ready:null},content_status:{phase:'up_to_date',note:'Current'},
 onboarding_progress:{complete:true,next:{kind:'already_complete'},plan:[],set_aside:[],summary:{graphics:'balanced'}}});
// The binding grid's rows, shaped as review-players.mjs answers setup_controller_mapping.
fixture.setup_controller_mapping={custom:false,rows:[...['A','B','X','Y','Z','Start'].map(x=>({key:'Buttons/'+x,label:x,binding:'`Button '+x+'`'})),
 ...['Up','Down','Left','Right'].flatMap(x=>['Main Stick','C-Stick','D-Pad'].map(g=>({key:g+'/'+x,label:g+' '+x,binding:'`'+x+'`'}))),...['L','R'].map(x=>({key:'Triggers/'+x,label:x,binding:'`Trigger '+x+'`'}))]};
fixture.devices={complete:true,pads:Array.from({length:4},(_,id)=>({device_string:`XInput/${id}/Gamepad`,name:'Gamepad',backend:'XInput',id}))};
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.jpg':'image/jpeg','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const f=path.resolve(ui,'.'+decodeURIComponent(new URL(req.url,'http://x').pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const base=`http://127.0.0.1:${server.address().port}`;
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
const errors=[];
async function open(file){
 const ctx=await browser.newContext({viewport:{width:1280,height:800}});
 await ctx.addInitScript(data=>{
  window.__TAURI__={event:{listen:async()=>()=>{}},core:{invoke:async(cmd)=>cmd in data?data[cmd]:null}};
  window.pads=[{id:'Test pad',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:18},()=>({pressed:false,value:0}))}];
  Object.defineProperty(navigator,'getGamepads',{value:()=>pads});document.hasFocus=()=>true;
  window.frames=[];window.clock=0;window.requestAnimationFrame=fn=>(frames.push(fn),frames.length);window.cancelAnimationFrame=()=>{};
  window.tick=()=>{clock+=16;frames.splice(0).forEach(fn=>fn(clock));};
 },fixture);
 const page=await ctx.newPage();page.on('pageerror',e=>errors.push(`${file}: ${e.message}`));
 await page.goto(`${base}/${file}`);await page.waitForFunction(()=>frames.length>0);await page.evaluate(()=>tick());
 return page;
}
const press=(page,b)=>page.evaluate(b=>{pads[0].buttons[b]={pressed:true,value:1};tick();pads[0].buttons[b]={pressed:false,value:0};tick();},b);
const controls=page=>page.evaluate(()=>[...document.querySelectorAll('#main a[href],#main button,#main input,#main select,#main summary')].filter(n=>n.getClientRects().length).map(n=>{const b=n.getBoundingClientRect();return {text:(n.getAttribute('aria-label')||n.textContent||'').trim().slice(0,60),x:Math.round(b.x),y:Math.round(b.y),w:Math.round(b.width)};}));
try{
 if(process.argv[2]==='probe'){for(const f of ['controls.html','stats.html']){const p=await open(f);await p.waitForTimeout(800);console.log(f,JSON.stringify(await controls(p)));}}
 const active=page=>page.evaluate(()=>(document.activeElement?.getAttribute('aria-label')||document.activeElement?.textContent||'').trim());
 {
  // Controllers binding grid: Down from the left stick's Left key goes to the D-pad group directly below,
  // never across the controller art to Buttons "Z" (which sits nearer, in the other column).
  const page=await open('controls.html');
  const left=page.getByRole('button',{name:'Assign Left stick left',exact:true});
  await left.waitFor();await left.focus();
  await press(page,13);
  const got=await active(page);
  assert.match(got,/^Assign D-Pad /,`Down from Left stick "Left" reaches the D-pad group (got "${got}")`);
  await page.getByRole('button',{name:'Assign Left stick right',exact:true}).focus();
  await press(page,13);
  assert.match(await active(page),/^Assign D-Pad /,'Down from Left stick "Right" reaches the D-pad group');
  await page.context().close();
 }
 {
  // Stats, Profile & sync: Up from "Send results now" stays in its column; "Sign out", beside it and a
  // little higher, is not "above".
  const page=await open('stats.html');
  await page.locator('details.stats-account').waitFor();
  await page.locator('details.stats-account').evaluate(d=>{d.open=true;});
  const send=page.getByRole('button',{name:'Send results now',exact:true});
  await send.waitFor();await send.focus();
  await press(page,12);
  const got=await active(page);
  assert.notEqual(got,'Sign out','Up from Send results now never goes sideways to Sign out');
  assert.match(got,/^Profile & sync/,`Up from Send results now goes up its column (got "${got}")`);
  await page.context().close();
 }
 assert.deepEqual(errors,[]);
 console.log('check-pad-columns: PASS');
}finally{await browser.close();server.close();}
