// Settings (spec §8.2, §7): the Dolphin card once, seven sliding tabs, and every change waiting in the
// save bar until Save changes, except volume and mute while a game is running.
//     node Tools/check-settings-page.mjs
import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync,mkdirSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const out=path.join(root,'target/glass-review');mkdirSync(out,{recursive:true});
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const f=path.resolve(ui,'.'+decodeURIComponent(new URL(req.url,'http://x').pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const base=`http://127.0.0.1:${server.address().port}`;
const VALUES={confirm_stop:'True',pause_unfocused:'False',osd_messages:'True',internal_resolution:'3',msaa:'1',video_backend:'D3D',sync_gpu:'False',vsync:'False',show_fps:'False',fullscreen:'True',show_ping:'False',always_on_top:'False',audio_backend:'Cubeb',volume:'40',muted:'False',audio_stretch:'False',audio_latency:'20',slot_a:'8',card_a:'',gci_a:'',slot_b:'255',card_b:'',gci_b:''};
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
const errors=[];
async function open(hash='',variant={},viewport={width:1280,height:800}){
 const ctx=await browser.newContext({viewport});
 await ctx.addInitScript(({values,variant})=>{
  window.calls=[];window.reject=null;
  const data={
   dolphin_preferences:{values,editable:true,live_editable:[],user_dir:'C:\\Users\\A\\Dolphin User'},
   settings_paths:{runtime:null,game:null,userDir:null,legacyRoot:null},
   launch_check:{ready:true,runtime:'C:\\rt\\Dolphin.exe',game:'C:\\g\\NSR.iso',user_dir_note:'u',problem:null},
   setup_graphics_options:{current:null,previous:null},update_policy:{automatic:true},
   paths:{install_root:'i',data_root:'d',saves_survive_uninstall:true,directories:[],channel:'beta',channel_note:'beta channel.'},
   content_status:{phase:'up_to_date',components:[],note:'Everything is current.'},
   launcher_look:{windowGlass:false,slowAnimations:false,glassSupported:true,glassApplied:false,glassNote:null},
   session_active:false,ui_notices:[],onboarding_progress:{complete:true,summary:{graphics:'balanced'}},...variant};
  window.data=data;
  window.__TAURI__={core:{invoke:async(cmd,args={})=>{
   calls.push({cmd,args:JSON.parse(JSON.stringify(args))});
   if(window.reject===cmd)throw 'Dolphin is open';
   if(cmd==='pick_path')return 'C:\\Games\\Other.iso';
   if(cmd==='settings_set_path'){const k={game:'game',runtime:'runtime',user_dir:'userDir',legacy_root:'legacyRoot'}[args.which];data.settings_paths={...data.settings_paths,[k]:args.value||null};return {which:args.which,value:args.value||null};}
   if(cmd==='dolphin_preference_set'){data.dolphin_preferences.values[args.key]=args.value;return null;}
   if(cmd==='window_glass_apply')return {...data.launcher_look,glassApplied:args.enabled};
   return cmd in data?data[cmd]:null;
  }}};
 },{values:VALUES,variant});
 const page=await ctx.newPage();page.on('pageerror',e=>errors.push(e.message));
 await page.goto(base+'/settings.html'+hash);await page.locator('.dolphin-card').waitFor();
 return page;
}
const sets=page=>page.evaluate(()=>calls.filter(c=>c.cmd==='dolphin_preference_set').map(c=>[c.args.key,c.args.value]));
try{
 {
  const page=await open();
  assert.equal(await page.locator('.dolphin-card').count(),1,'one Dolphin card, not one per tab');
  await page.getByRole('button',{name:'Open Dolphin',exact:true}).waitFor();
  assert.deepEqual(await page.locator('.tabs .tab').allInnerTexts(),['General','Graphics','Display','Audio','Saves','Updates','Files & storage']);
  const ind=async()=>{await page.waitForTimeout(450);return page.evaluate(()=>{const i=document.querySelector('.tab-ind').getBoundingClientRect(),t=document.querySelector('.tabs .tab[aria-pressed="true"]').getBoundingClientRect();return Math.max(Math.abs(i.x-t.x),Math.abs(i.width-t.width));});};
  assert.ok(await ind()<1.5,'the highlight sits on General');
  await page.getByRole('button',{name:'Audio',exact:true}).click();
  assert.ok(await ind()<1.5,'the highlight slid to Audio');
  assert.ok(await page.locator('#screen').evaluate(n=>n.getBoundingClientRect().width)<=942.5,'form column ≤ 942px (mockup .inner 1014 minus its padding)');
  await page.getByRole('button',{name:'General',exact:true}).click();
  const pause=()=>page.getByRole('switch',{name:'Pause when unfocused'});
  assert.equal(await pause().getAttribute('aria-checked'),'false');
  await pause().click();
  await page.locator('.save-lane.open').waitFor();
  assert.match(await page.locator('.save-bar').innerText(),/1 unsaved change[\s\S]*Pause when unfocused/);
  assert.equal(await pause().getAttribute('aria-checked'),'true','the switch shows the pending value');
  assert.deepEqual(await sets(page),[],'nothing written before Save');
  await page.waitForTimeout(400);
  const edge=await page.evaluate(()=>({main:document.getElementById('main').getBoundingClientRect().bottom,lane:document.getElementById('save-lane').getBoundingClientRect().top}));
  assert.ok(Math.abs(edge.main-edge.lane)<1,'the bar never covers content');
  await page.getByRole('button',{name:'Discard',exact:true}).click();
  await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
  assert.equal(await pause().getAttribute('aria-checked'),'false','Discard puts it back');
  await pause().click();
  await page.getByRole('switch',{name:'Game notifications'}).click();
  assert.match(await page.locator('.save-bar').innerText(),/2 unsaved changes[\s\S]*Game notifications/);
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
  assert.deepEqual(await sets(page),[['pause_unfocused','true'],['osd_messages','false']],'both written, in order');
  await page.evaluate(()=>window.reject='dolphin_preference_set');
  await page.getByRole('switch',{name:'Confirm before stopping'}).click();
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await page.locator('.save-bar [role=alert]').waitFor();
  assert.match(await page.locator('.save-bar [role=alert]').innerText(),/Confirm before stopping: Dolphin is open/);
  assert.equal(await page.locator('.save-lane.open').count(),1,'a failed change stays pending');
  await page.evaluate(()=>window.reject=null);
  await page.getByRole('button',{name:'Discard',exact:true}).click();
  await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
  await page.context().close();
 }
 {
  const page=await open('#graphics');
  await page.getByLabel('Graphics preset',{exact:true}).selectOption('performance');
  assert.match(await page.locator('.save-bar').innerText(),/Graphics preset/);
  assert.equal(await page.evaluate(()=>calls.some(c=>c.cmd==='setup_save_graphics_options')),false);
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await page.waitForFunction(()=>localStorage.getItem('h:preset')==='performance');
  await page.context().close();
 }
 {
  const page=await open('#audio',{dolphin_preferences:{values:VALUES,editable:false,live_editable:['volume','muted'],readonly_reason:'Close Dolphin to change these.',user_dir:'x'}});
  await page.getByRole('slider',{name:'Game volume',exact:true}).evaluate(n=>{n.value='37';n.dispatchEvent(new Event('change',{bubbles:true}));});
  await page.waitForFunction(()=>calls.some(c=>c.cmd==='dolphin_preference_set'&&c.args.key==='volume'&&c.args.value==='37'));
  assert.equal(await page.locator('.save-lane.open').count(),0,'live volume never waits for the bar');
  await page.context().close();
 }
 {
  const page=await open('#files');
  const field=page.getByLabel('Game image',{exact:true});
  assert.match(await field.inputValue(),/NSR\.iso/,'shows the detected image');
  assert.equal(await page.locator('#path-game-auto').innerText(),'Auto');
  await page.getByRole('button',{name:'Change Game image',exact:true}).click();
  await page.locator('.save-lane.open').waitFor();
  assert.match(await field.inputValue(),/Other\.iso/,'shows the pending path');
  assert.equal(await page.evaluate(()=>calls.some(c=>c.cmd==='settings_set_path')),false);
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await page.waitForFunction(()=>calls.some(c=>c.cmd==='settings_set_path'&&c.args.which==='game'&&c.args.value==='C:\\Games\\Other.iso'));
  await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
  assert.equal(await page.locator('#path-game-auto').count(),0,'a chosen path is not Auto');
  await page.screenshot({path:path.join(out,'settings-files-1280.png')});
  await page.context().close();
 }
 {
  // A path can still be typed or pasted (spec §10): Enter stages it, quotes dropped; leaving a field stages it too.
  const page=await open('#files');
  const field=page.getByLabel('Game image',{exact:true});
  await field.fill('"C:\\Pasted\\NSR.iso"');await field.press('Enter');
  await page.locator('.save-lane.open').waitFor();
  assert.match(await page.locator('.save-bar').innerText(),/1 unsaved change[\s\S]*Game image/);
  assert.equal(await page.getByLabel('Game image',{exact:true}).inputValue(),'C:\\Pasted\\NSR.iso','quotes dropped');
  await page.getByLabel('Existing install',{exact:true}).fill('D:\\Old Reborn');
  await page.getByLabel('Emulator',{exact:true}).focus();
  assert.match(await page.locator('.save-bar').innerText(),/2 unsaved changes[\s\S]*Existing install/,'leaving a typed field stages it');
  assert.equal(await page.evaluate(()=>calls.some(c=>c.cmd==='settings_set_path')),false,'nothing saved before Save');
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
  assert.deepEqual(await page.evaluate(()=>calls.filter(c=>c.cmd==='settings_set_path').map(c=>[c.args.which,c.args.value])),[['game','C:\\Pasted\\NSR.iso'],['legacy_root','D:\\Old Reborn']]);
  await page.context().close();
 }
 {
  const page=await open('',{},{width:800,height:560});
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no sideways scroll at 800x560');
  await page.screenshot({path:path.join(out,'settings-800.png')});
  await page.context().close();
 }
 assert.deepEqual(errors,[]);
 console.log('check-settings-page: PASS');
}finally{await browser.close();server.close();}
