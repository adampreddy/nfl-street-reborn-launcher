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
const fixOut=path.join(root,'target/p2-fix');mkdirSync(fixOut,{recursive:true});
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const f=path.resolve(ui,'.'+decodeURIComponent(new URL(req.url,'http://x').pathname));if(!f.startsWith(ui+path.sep))throw 0;res.setHeader('Content-Type',types[path.extname(f)]||'application/octet-stream');res.end(readFileSync(f));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const base=`http://127.0.0.1:${server.address().port}`;
const VALUES={confirm_stop:'True',pause_unfocused:'False',osd_messages:'True',internal_resolution:'3',msaa:'1',video_backend:'D3D',sync_gpu:'False',vsync:'False',show_fps:'False',fullscreen:'True',show_ping:'False',always_on_top:'False',audio_backend:'Cubeb',volume:'40',muted:'False',audio_stretch:'False',audio_latency:'20',slot_a:'8',card_a:'',gci_a:'',slot_b:'255',card_b:'',gci_b:''};
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER||'msedge',headless:true});
const errors=[];
async function open(hash='',variant={},viewport={width:1280,height:800},pad=false){
 const ctx=await browser.newContext({viewport});
 if(pad)await ctx.addInitScript(()=>{
  window.pads=[{id:'Test pad',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:18},()=>({pressed:false,value:0}))}];
  Object.defineProperty(navigator,'getGamepads',{value:()=>pads});document.hasFocus=()=>true;
  window.frames=[];window.clock=0;window.requestAnimationFrame=fn=>(frames.push(fn),frames.length);window.cancelAnimationFrame=()=>{};
  window.tick=()=>{clock+=16;frames.splice(0).forEach(fn=>fn(clock));};
 });
 await ctx.addInitScript(({values,variant})=>{
  window.calls=[];window.reject=null;window.hold=null;window.unverifiedPath=false;
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
   if(window.hold===cmd){window.hold=null;await new Promise(r=>{window.release=r;});}
   if(window.reject===cmd)throw 'Dolphin is open';
   if(cmd==='settings_set_path'&&window.unverifiedPath)return {unverified:{headline:'This is not a Reborn build.',detail:'Rosters and faces will not match.',play_anyway:'Play it anyway',repair:'Repair my game'}};
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
  assert.equal(await page.locator('#path-legacyRoot-auto').count(),0,'nothing found for Existing install, so no Auto');
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
  // Live audio leaves the draft (spec §10): a volume staged while Dolphin was closed must not be written
  // over the value set live once Dolphin opened.
  const page=await open('#audio');
  const vol=()=>page.getByRole('slider',{name:'Game volume',exact:true});
  await vol().evaluate(n=>{n.value='50';n.dispatchEvent(new Event('change',{bubbles:true}));});
  await page.locator('.save-lane.open').waitFor();
  await page.evaluate(()=>{data.dolphin_preferences={...data.dolphin_preferences,editable:false,live_editable:['volume','muted'],readonly_reason:'Close Dolphin to change these.'};document.dispatchEvent(new Event('session-state'));});
  await page.waitForFunction(()=>document.getElementById('dolphin-status')?.textContent.includes('Volume and mute change as you set them'));
  await vol().evaluate(n=>{n.value='70';n.dispatchEvent(new Event('change',{bubbles:true}));});
  await page.waitForFunction(()=>calls.some(c=>c.cmd==='dolphin_preference_set'&&c.args.key==='volume'&&c.args.value==='70'));
  await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
  assert.equal(await vol().inputValue(),'70','the slider keeps the live value, not the staged one');
  assert.deepEqual(await sets(page),[['volume','70']],'the staged 50 is never written');
  await page.context().close();
 }
 {
  // A Discard that cannot put a preview back says so and keeps the change (P2-12): nothing still
  // previewed is shown as discarded.
  const page=await open('#display');
  const glass=()=>page.getByRole('switch',{name:'Window glass'});
  await glass().click();
  await page.locator('.save-lane.open').waitFor();
  await page.evaluate(()=>window.reject='window_glass_apply');
  await page.getByRole('button',{name:'Discard',exact:true}).click();
  await page.locator('.save-bar [role=alert]').waitFor();
  assert.match(await page.locator('.save-bar [role=alert]').innerText(),/Window glass: Dolphin is open/);
  assert.deepEqual(await page.evaluate(()=>{const l=document.getElementById('save-lane');return {open:l.classList.contains('open'),inert:l.inert};}),{open:true,inert:false},'the bar stays open and usable');
  assert.match(await page.locator('.save-bar').innerText(),/1 unsaved change[\s\S]*Window glass/,'Window glass is still listed');
  assert.equal(await glass().getAttribute('aria-checked'),'true','the switch shows what is still previewed');
  await page.evaluate(()=>window.reject=null);
  await page.getByRole('button',{name:'Discard',exact:true}).click();
  await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
  assert.equal(await glass().getAttribute('aria-checked'),'false','a second Discard puts it back');
  await page.context().close();
 }
 {
  // The pad on Settings (spec §6, §7): tabs → a row, a change opens the bar, Down reaches Save changes,
  // Up goes back, and B twice with changes pending lands on the bar instead of leaving.
  const page=await open('#general',{},{width:1280,height:800},true);
  await page.evaluate(()=>tick());
  const press=b=>page.evaluate(b=>{pads[0].buttons[b]={pressed:true,value:1};tick();pads[0].buttons[b]={pressed:false,value:0};tick();},b);
  const at=()=>page.evaluate(()=>{const a=document.activeElement;return {id:a?.id||'',text:(a?.textContent||'').trim(),row:!!a?.closest('.pref-row'),tab:!!a?.closest('.tabs'),main:!!document.getElementById('main').contains(a),lane:!!document.getElementById('save-lane').contains(a),nav:!!a?.closest('.shell-nav')};});
  await page.locator('.tabs .tab[aria-pressed="true"]').focus();
  assert.equal((await at()).tab,true,'starts on the tabs');
  for(let i=0;i<3&&!(await at()).row;i++)await press(13);
  const first=await at();
  assert.equal(first.row,true,'D-pad Down from the tabs reaches a row '+JSON.stringify(first));
  await press(0);
  await page.locator('.save-lane.open').waitFor();
  assert.equal((await at()).id,first.id,'focus stays on the control that changed');
  for(let i=0;i<12&&(await at()).text!=='Save changes';i++)await press(13);
  assert.deepEqual([(await at()).lane,(await at()).text],[true,'Save changes'],'D-pad Down reaches Save changes');
  await press(12);
  assert.equal((await at()).main,true,'Up goes back into the page');
  await press(1);
  assert.equal((await at()).nav,true,'B from the page goes to the sidebar');
  await press(1);
  assert.deepEqual([(await at()).lane,(await at()).text],[true,'Save changes'],'B again with changes pending lands on the save bar');
  assert.ok(page.url().includes('/settings.html'),'and never leaves the page');
  assert.equal(await page.locator('.save-lane.open').count(),1,'the change is still pending');
  // P3-14: Settings' draft is page-local, so its lane is never marked keeps and a sidebar click is refused.
  assert.equal(await page.locator('#save-lane[data-keeps]').count(),0,'the Settings lane is not keeps');
  assert.equal(await page.evaluate(sel=>{let seen=null;const l=e=>{seen=e.defaultPrevented;e.preventDefault();};window.addEventListener('click',l);document.querySelector(sel).click();window.removeEventListener('click',l);return seen;},'.shell-nav a[href="mods.html"]'),null,'a sidebar click with a Settings change pending is intercepted');
  assert.ok(page.url().includes('/settings.html'),'still on Settings');
  await page.context().close();
 }
 {
  // The Tier 2 choice: Esc (the shell's B does the same: cancel, then close) is Cancel. The next draw must
  // not reopen it, and the game path change stays pending.
  const page=await open('#files');
  await page.evaluate(()=>window.unverifiedPath=true);
  await page.getByRole('button',{name:'Change Game image',exact:true}).click();
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await page.locator('#unverified-game[open]').waitFor();
  await page.keyboard.press('Escape');
  await page.waitForFunction(()=>!document.querySelector('#unverified-game[open]'));
  await page.getByRole('button',{name:'Updates',exact:true}).click();
  await page.getByRole('button',{name:'Files & storage',exact:true}).click();
  await page.waitForTimeout(150);
  assert.equal(await page.locator('#unverified-game[open]').count(),0,'Esc keeps the Tier 2 choice closed after the next draw');
  assert.equal(await page.locator('.save-lane.open').count(),1,'the change stays pending after Esc');
  // B through the shell's back(): cancel, then close. Same result.
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await page.locator('#unverified-game[open]').waitFor();
  await page.evaluate(()=>{const d=document.getElementById('unverified-game');if(d.dispatchEvent(new Event('cancel',{cancelable:true})))d.close();});
  await page.getByRole('button',{name:'Updates',exact:true}).click();
  await page.waitForTimeout(150);
  assert.equal(await page.locator('#unverified-game[open]').count(),0,'B (cancel + close) keeps the Tier 2 choice closed after the next draw');
  await page.context().close();
 }
 {
  // Save holds the page (P2 final): while it runs, #screen is aria-busy and no action can start, even
  // after something redraws; when it ends, or fails, the page is usable again.
  const page=await open();
  const openDolphin=()=>page.getByRole('button',{name:'Open Dolphin',exact:true});
  await page.getByRole('switch',{name:'Pause when unfocused'}).click();
  await page.evaluate(()=>window.hold='dolphin_preference_set');
  await page.getByRole('button',{name:'Save changes',exact:true}).click();
  await page.waitForFunction(()=>typeof window.release==='function');
  assert.equal(await page.locator('#screen').getAttribute('aria-busy'),'true','Save marks the page busy');
  await page.evaluate(()=>{data.dolphin_preferences={...data.dolphin_preferences,readonly_reason:'changed'};document.dispatchEvent(new Event('session-state'));});
  await page.waitForTimeout(150);
  assert.equal(await openDolphin().isDisabled(),true,'Open Dolphin cannot start mid-save');
  assert.equal(await page.evaluate(()=>calls.some(c=>c.cmd==='open_runtime')),false);
  assert.notEqual(await openDolphin().evaluate(n=>getComputedStyle(n).pointerEvents),'none','a disabled .btn keeps pointer events, so its tooltip still shows');
  await openDolphin().click({force:true});await page.waitForTimeout(100);
  assert.equal(await page.evaluate(()=>calls.some(c=>c.cmd==='open_runtime')),false,'a disabled .btn still cannot be activated');
  await page.evaluate(()=>{window.reject='dolphin_preference_set';window.release();});
  await page.locator('.save-bar [role=alert]').waitFor();
  assert.equal(await page.locator('#screen').getAttribute('aria-busy'),null,'a failed save leaves the page not busy');
  assert.equal(await openDolphin().isEnabled(),true,'a failed save leaves the page usable');
  await page.evaluate(()=>window.reject=null);
  await openDolphin().click();
  await page.waitForFunction(()=>calls.some(c=>c.cmd==='open_runtime'));
  await page.getByRole('button',{name:'Discard',exact:true}).click();
  await page.waitForFunction(()=>!document.querySelector('.save-lane.open'));
  assert.equal(await page.locator('#screen').getAttribute('aria-busy'),null,'Discard leaves the page not busy');
  await page.context().close();
 }
 {
  // The smallest window (spec §4.3; about 780x530 inside the 800x560 frame): all seven tabs whole, and the
  // sidebar fits with no scrollbar, at 780x540 and with headroom below it (500: the old rhythm needed 512).
  for(const height of [540,500]){
   const page=await open('#files',{},{width:780,height});
   await page.waitForTimeout(500);
   const fit=await page.evaluate(()=>{const bar=document.querySelector('.tabs'),last=bar.querySelector('.tab:last-of-type'),b=bar.getBoundingClientRect(),l=last.getBoundingClientRect(),nav=document.querySelector('.shell-nav');
    return {last:last.textContent,lastRight:l.right,barRight:b.right,lastLeft:l.left,barLeft:b.left,scrolls:bar.scrollWidth>bar.clientWidth,navScroll:nav.scrollHeight,navClient:nav.clientHeight,page:document.documentElement.scrollWidth<=innerWidth};});
   assert.equal(fit.last,'Files & storage');
   assert.ok(fit.lastRight<=fit.barRight+0.5&&fit.lastLeft>=fit.barLeft-0.5,`780x${height}: the last tab is fully visible ${JSON.stringify(fit)}`);
   assert.equal(fit.page,true,`780x${height}: no sideways page scroll`);
   assert.ok(fit.navScroll<=fit.navClient,`780x${height}: the sidebar has no vertical overflow ${JSON.stringify(fit)}`);
   const ind=await page.evaluate(()=>{const i=document.querySelector('.tab-ind').getBoundingClientRect(),t=document.querySelector('.tabs .tab[aria-pressed="true"]').getBoundingClientRect();return Math.max(Math.abs(i.x-t.x),Math.abs(i.width-t.width));});
   assert.ok(ind<1.5,`780x${height}: the highlight sits on Files & storage (${ind})`);
   if(height===540)await page.screenshot({path:path.join(fixOut,'settings-files-780x540.png')});
   await page.context().close();
  }
 }
 {
  // The highlight sits exactly under the active tab (indicator box vs tab box, 1.5px) at 800x560 and 1280x800: opened
  // on Files & storage, and after picking it from General (the strip may scroll to show it).
  const box=page=>page.evaluate(()=>{const i=document.querySelector('.tab-ind').getBoundingClientRect(),t=document.querySelector('.tabs .tab[aria-pressed="true"]').getBoundingClientRect();
   return {d:Math.max(Math.abs(i.left-t.left),Math.abs(i.right-t.right),Math.abs(i.top-t.top),Math.abs(i.bottom-t.bottom)),i:[i.left,i.right,i.top,i.bottom].map(Math.round),t:[t.left,t.right,t.top,t.bottom].map(Math.round),tab:document.querySelector('.tabs .tab[aria-pressed="true"]').textContent};});
  const settled=async(page,what)=>{await page.waitForTimeout(700);const b=await box(page);assert.ok(b.d<1.5,`${what}: the highlight sits under ${b.tab} ${JSON.stringify(b)}`);};
  for(const viewport of [{width:800,height:560},{width:1280,height:800}]){
   const v=`${viewport.width}x${viewport.height}`;
   let page=await open('#files',{},viewport);
   await settled(page,`${v} opened on Files & storage`);
   await page.context().close();
   page=await open('',{},viewport);
   await settled(page,`${v} General`);
   await page.getByRole('button',{name:'Files & storage',exact:true}).click();
   await settled(page,`${v} picked Files & storage`);
   await page.getByRole('button',{name:'Graphics',exact:true}).click();
   await settled(page,`${v} back to Graphics`);
   await page.context().close();
  }
  // The window resized across the 900px rule (tab padding 14 -> 10px): the highlight follows the tab's new box.
  const page=await open('#files',{},{width:1280,height:800});
  await settled(page,'1280x800 on Files & storage');
  await page.setViewportSize({width:800,height:560});
  await settled(page,'resized to 800x560');
  await page.setViewportSize({width:1280,height:800});
  await settled(page,'resized back to 1280x800');
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
