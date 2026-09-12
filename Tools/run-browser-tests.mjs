import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';
import assert from 'node:assert/strict';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const ui = path.join(root, 'crates/reborn-ui/ui');
const out = path.resolve(process.argv[2] || path.join(root, 'target/browser-review'));
mkdirSync(out,{recursive:true});
const source = readFileSync(path.join(root,'Tools/check-shell-pages.mjs'),'utf8');
const fixture = vm.runInNewContext(source.slice(source.indexOf('const FULL ='),source.indexOf('// Every field null'))+'\nFULL');
Object.assign(fixture, {
 session_active: false,
 settings_paths: {runtime:null,game:null,userDir:null,legacyRoot:null},
 update_policy: {automatic:false,source:'settings'},
 content_status: {phase:'up_to_date',origin:'https://files.test/',release:'3.0.0 sequence 3',components:[],download_bytes:0,download_files:0,repair_only:false,note:'Everything is current.',installed:null},
 content_check: {phase:'up_to_date',origin:'https://files.test/',release:'3.0.0 sequence 3',components:[],download_bytes:0,download_files:0,repair_only:false,note:'Everything is current.',installed:null},
 onboarding_progress: {complete:true,next:{kind:'already_complete'},plan:[],set_aside:[],summary:{graphics:'balanced'}},
});
fixture.mod_packages = {packages_root:'C:/Reborn/mods',packages:[{id:'football-red',version:'1.0',name:'Red football',active:true,files:1,bytes:1048576},{id:'football-black',version:'1.0',name:'Black football',active:false,files:1,bytes:1048576},{id:'textures',version:'2.1',name:'Reborn textures',active:true,files:54,bytes:18000000}],activation:[{id:'football-red',version:'1.0'},{id:'textures',version:'2.1'}],unreadable:[],exclusive:[{members:['football-red','football-black'],computed:true,example_path:'football.png'}],managed_by_controls:[]};
fixture.controls_options=[{slug:'xbox',label:'Xbox',suggested:true,describes:'A, B, X and Y prompts.',pack:'Xbox Controls'},{slug:'playstation',label:'PlayStation',suggested:false,describes:'Cross, circle, square and triangle prompts.',pack:'PS Controls'},{slug:'gamecube',label:'GameCube',describes:'Original game prompts.',pack:null}];
const types={'.svg':'image/svg+xml','.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.jpg':'image/jpeg','.woff2':'font/woff2'};
const server=createServer((req,res)=>{
 try {
  const pathname=decodeURIComponent(new URL(req.url,'http://localhost').pathname);
  const file=path.resolve(ui,'.'+(pathname==='/'?'/index.html':pathname));
  if(!file.startsWith(ui+path.sep)) throw new Error('outside');
  res.writeHead(200,{'Content-Type':types[path.extname(file)]||'application/octet-stream'});res.end(readFileSync(file));
 } catch {res.writeHead(404);res.end('Not found');}
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const url=`http://127.0.0.1:${server.address().port}`;
const browser=await chromium.launch({channel:process.env.REBORN_TEST_BROWSER || 'msedge',headless:true});
const context=await browser.newContext({viewport:{width:1280,height:800}});
await context.addInitScript(data=>{
 window.__calls=[]; window.__fixture=data;
 const stage=new URL(location.href).searchParams.get('stage');
 if(stage) {
  const stages=['welcome','game_verification','controller_setup','controls_profile','system_check','updates','preparation','completion'];
  data.onboarding_progress={complete:false,next:{kind:'show',stage,position:stages.indexOf(stage)+1,total:stages.length},plan:stages.map(s=>({stage:s,satisfied:stages.indexOf(s)<stages.indexOf(stage)})),set_aside:[],summary:{},preparation_remaining:[],tasks_done:[]};
 }
 if(new URL(location.href).searchParams.has('missing')) data.launch_check={ready:false,problem:'No game image was found.'};
 window.__TAURI__={core:{invoke:async(cmd,args={})=>{
  window.__calls.push({cmd,args});
  if(window.__delayCommand===cmd) await new Promise(resolve=>{window.__releaseDelayed=resolve;});
  if(window.__reject===cmd) throw new Error('Test failure: '+cmd);
  if(cmd==='settings_paths') return JSON.parse(localStorage.getItem('qa:settings')||'null')||data.settings_paths;
  if(cmd==='settings_set_path') {
   const prefs=JSON.parse(localStorage.getItem('qa:settings')||'null')||{...data.settings_paths};
   prefs[{runtime:'runtime',game:'game',user_dir:'userDir',legacy_root:'legacyRoot'}[args.which]]=args.value||null;
   localStorage.setItem('qa:settings',JSON.stringify(prefs));return {which:args.which,value:args.value};
  }
  if(cmd==='pick_path') return 'C:\\Games\\Reborn\\NSR.iso';
  if(cmd==='launch') return new Promise(resolve=>{data.session_active=true;window.__finishLaunch=r=>{data.session_active=false;resolve(r);};});
  if(cmd==='controls_set') {data.controls_current={profile:args.profile,label:args.profile,source:'settings'};return {label:args.profile,synced:{note:'Button prompts updated.'}};}
  if(cmd==='mods_toggle') {data.mod_packages.packages.forEach(p=>{if(p.id===args.id)p.active=args.on;else if(args.on&&p.id.startsWith('football'))p.active=false;});return {synced:true,note:'Mods updated.'};}
  if(cmd==='mods_sync') return {synced:true,note:'Your mods are ready.',entries:55,tree:'rebuilt',missing:[]};
  if(cmd==='onboarding_accept_migration') {data.onboarding_progress.next.stage='migration_copy';return {outcome:'recorded'};}
  if(cmd==='migration_apply') {data.onboarding_progress.next.stage='system_check';return {steps:[],total_bytes:24};}
  if(cmd==='setup_install_info') return data.setup_info||{mode:'migrate',root:'C:/Reborn',games:['C:/Reborn/Games/NSR.iso'],settings_found:true,installed_release:{release:'2.1',game:'week10',textures:'week10'},memory_cards:['C:/Reborn/User/GC/MemoryCardA.USA.raw']};
  if(cmd==='setup_install_choice') {data.setup_info={mode:args.mode,memory_cards:[]};return;}
  if(cmd==='setup_import_game') return {path:'C:/Reborn/library/imported/test/NSR.iso',patched:true};
  if(cmd==='setup_use_existing') return;
  if(cmd==='setup_graphics_options') return {current:null,previous:null};
  if(cmd==='setup_save_graphics_options') return;
  if(cmd==='setup_migrate_install'){data.onboarding_progress.next.stage='controller_setup';return {imported:true};}
  if(cmd==='onboarding_acknowledge') {data.onboarding_progress.next.stage='game_verification';return {outcome:'recorded'};}
  if(cmd==='onboarding_choose_controls') {data.onboarding_progress.summary.controls_profile=args.profile;data.onboarding_progress.next.stage='system_check';return {outcome:'recorded'};}
  if(cmd==='onboarding_choose_graphics') {data.onboarding_progress.summary.graphics=args.preset;data.onboarding_progress.next.stage='updates';return {outcome:'recorded'};}
  if(cmd==='onboarding_choose_updates') {data.onboarding_progress.next.stage='preparation';return {outcome:'recorded'};}
  if(cmd==='onboarding_prepared') {data.onboarding_progress.next.stage='completion';return {outcome:'recorded'};}
  if(cmd==='setup_hardware_info') return {description:'NVIDIA GeForce RTX 4070 · 32 GB RAM',recommended:'quality'};
  if(cmd==='setup_controller_mapping') return {custom:false,rows:[...['A','B','X','Y','Z','Start'].map(x=>({key:'Buttons/'+x,label:x,binding:'`Button '+x+'`'})),...['Up','Down','Left','Right'].flatMap(x=>['Main Stick','C-Stick','D-Pad'].map(g=>({key:g+'/'+x,label:g+' '+x,binding:'`'+x+'`'}))),...['L','R'].map(x=>({key:'Triggers/'+x,label:x,binding:'`Trigger '+x+'`'}))]};
  if(cmd==='setup_save_mapping') return;
  if(cmd==='onboarding_run_task') return {task:args.task,step:{outcome:'recorded'}};
  if(cmd==='onboarding_finish') {data.onboarding_progress.complete=true;return {outcome:'recorded'};}
  if(cmd==='onboarding_back') return {to:args.stage==='controls_profile'?'controller_setup':null};
  if(cmd==='onboarding_reach') return {outcome:'recorded'};
  if(cmd==='onboarding_confirm_game') {data.onboarding_progress.next.stage='controller_setup';return {outcome:'recorded'};}
  if(cmd==='onboarding_choose_controller') {data.onboarding_progress.next.stage='controls_profile';return {outcome:'recorded'};}
  if(cmd in data) return data[cmd];
  throw new Error('No test fixture for '+cmd);
 }}};
},fixture);
const page=await context.newPage();
const errors=[];page.on('pageerror',e=>errors.push(e.message));
await page.goto(url);await page.getByRole('button',{name:'Play now'}).waitFor();await page.evaluate(()=>document.fonts.ready);
await page.screenshot({path:path.join(out,'reborn-launcher-home.png')});
await page.getByRole('button',{name:'Game details',exact:true}).click();
await page.getByRole('dialog').getByRole('link',{name:/Graphics Balanced/}).click();
await page.getByLabel('Graphics preset',{exact:true}).selectOption('quality');
await page.waitForFunction(()=>localStorage.getItem('h:preset')==='quality');
await page.goto(url);await page.getByRole('button',{name:'Play now'}).waitFor();
await page.bringToFront();
await page.evaluate(()=>{
 window.__testPad={id:'Xbox test controller',connected:true,mapping:'standard',axes:[0,0],buttons:Array.from({length:17},()=>({pressed:false}))};
 Object.defineProperty(navigator,'getGamepads',{configurable:true,value:()=>[window.__testPad]});
 document.querySelector('#play-now').focus();
});
await page.waitForTimeout(100);
await page.evaluate(()=>window.__testPad.buttons[15].pressed=true);
await page.waitForTimeout(80);
await page.evaluate(()=>window.__testPad.buttons[15].pressed=false);
assert.notEqual(await page.evaluate(()=>document.activeElement?.id),'play-now','D-pad moves focus');
assert.match(await page.locator('#input-hints').innerText(),/A\s+Select/);
await page.evaluate(()=>{window.__testPad.connected=false;});
await page.getByRole('button',{name:'Play now'}).click();
await page.getByRole('button',{name:'Session active'}).waitFor();
assert.equal(await page.getByRole('button',{name:'Session active'}).isDisabled(),true);
await page.evaluate(()=>window.__finishLaunch({started:false,message:'The emulator could not start.'}));
await page.getByRole('alert').filter({hasText:'The emulator could not start.'}).waitFor();
assert.equal(await page.getByRole('button',{name:'Play now'}).isEnabled(),true);
await page.goto(url+'/settings.html');await page.getByRole('button',{name:'Files & storage',exact:true}).click();await page.getByLabel('Game image',{exact:true}).waitFor();
await page.screenshot({path:path.join(out,'reborn-launcher-settings.png')});
await page.getByRole('button',{name:'Browse for Game image',exact:true}).click();
await page.waitForFunction(()=>localStorage.getItem('h:game')?.includes('NSR.iso'));
await page.reload();await page.getByRole('button',{name:'Files & storage',exact:true}).click();await page.getByLabel('Game image',{exact:true}).waitFor();
assert.match(await page.getByLabel('Game image',{exact:true}).inputValue(),/NSR.iso/);
await page.evaluate(()=>window.__reject='settings_set_path');
await page.getByLabel('Game image',{exact:true}).fill('C:\\invalid.iso');
await page.getByRole('button',{name:'Save Game image',exact:true}).click();
await page.getByRole('alert').waitFor();
assert.match(await page.evaluate(()=>localStorage.getItem('h:game')),/NSR.iso/);
// Four-player mapping and live input are covered in review-players.mjs.
await page.goto(url+'/mods.html');await page.getByText('Red football',{exact:true}).waitFor();
await page.screenshot({path:path.join(out,'reborn-launcher-mods.png')});
await page.getByRole('button',{name:'Enable Black football',exact:true}).click();
await page.waitForFunction(()=>window.__fixture.mod_packages.packages.find(p=>p.id==='football-black').active);
for(const viewport of [{width:960,height:600},{width:800,height:600},{width:1920,height:1080}]) {
 await page.setViewportSize(viewport);await page.goto(url+'/index.html');await page.getByRole('button',{name:'Play now'}).waitFor();
 assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'horizontal overflow');
 await page.screenshot({path:path.join(out,`reborn-launcher-${viewport.width}.png`)});
}
await page.setViewportSize({width:1280,height:800});
await page.goto(url+'/onboarding.html');await page.getByRole('link',{name:'Back to Play'}).waitFor();
await page.getByText('Set up another installation',{exact:true}).click();
assert.equal(await page.getByRole('button',{name:'Restart setup'}).isDisabled(),true);
await page.screenshot({path:path.join(out,'reborn-launcher-setup.png')});
await page.goto(url+'/onboarding.html?stage=welcome');
await page.getByRole('heading',{name:'Welcome to NFL Street Reborn',exact:true}).waitFor();
assert.equal(await page.getByRole('link',{name:'Finish later'}).isVisible(),true);
assert.equal(await page.locator('#next').isDisabled(),true);
await page.getByRole('radio',{name:/Start fresh/}).check();
await page.screenshot({path:path.join(out,'reborn-setup-welcome.png')});
await page.locator('#next').click();
await page.getByRole('button',{name:'Browse for NFL Street 2'}).click();
await page.getByText('Patched and verified. Continue to your controller.',{exact:true}).waitFor();
await page.locator('#next').click();
await page.getByRole('heading',{name:'Your controller',exact:true}).waitFor();
assert.ok(await page.evaluate(()=>window.__calls.some(c=>c.cmd==='setup_import_game')));
await page.goto(url+'/onboarding.html?stage=game_verification');
await page.getByRole('heading',{name:'Make yourself at home'}).waitFor();
await page.getByText('Change detected files',{exact:true}).click();await page.getByRole('button',{name:'Browse for a game file',exact:true}).click();
await page.evaluate(()=>window.__reject='setup_migrate_install');
await page.getByRole('button',{name:'Import and continue',exact:true}).click();
await page.locator('.step-error').filter({hasText:'Test failure: setup_migrate_install'}).waitFor();
assert.match(await page.getByRole('combobox',{name:'Reborn game file'}).inputValue(),/NSR.iso/);
await page.evaluate(()=>window.__reject=null);
await page.getByRole('button',{name:'Import and continue',exact:true}).click();
await page.getByRole('heading',{name:'Your controller',exact:true}).waitFor();
await page.goto(url+'/onboarding.html?stage=system_check');
await page.getByRole('heading',{name:'Choose your graphics setting',exact:true}).waitFor();
assert.equal(await page.locator('input[name=preset]').count(),3);
await page.screenshot({path:path.join(out,'reborn-onboarding-graphics.png')});
await page.goto(url+'/onboarding.html?stage=controller_setup');
await page.getByRole('button',{name:'Scan again'}).waitFor();
assert.ok(await page.evaluate(()=>window.__calls.some(c=>c.cmd==='devices')),'controller setup scans automatically');
await page.getByRole('combobox',{name:'Your controller',exact:true}).selectOption('0');
assert.equal(await page.locator('#next').isEnabled(),true);
await page.getByRole('button',{name:'Scan again'}).click();
await page.getByRole('combobox',{name:'Your controller',exact:true}).selectOption('0');
// First run keeps the bindings behind "Customize controls" since 2026-09-11.
await page.getByText('Customize controls',{exact:true}).click();
await page.getByRole('button',{name:'Assign A',exact:true}).waitFor();
assert.equal(await page.evaluate(()=>document.body.dataset.controllerTesting),'setup');
await page.screenshot({path:path.join(out,'reborn-setup-controller.png')});
await page.getByRole('button',{name:'Assign A',exact:true}).click();
// The typed input sits behind "Type a Dolphin input name instead" since 2026-09-11.
await page.getByText('Type a Dolphin input name instead',{exact:true}).click();
await page.getByRole('textbox',{name:'A binding',exact:true}).fill('`Button Y`');
await page.getByRole('button',{name:'Use binding',exact:true}).click();
await page.getByRole('status').filter({hasText:'A saved.'}).waitFor();
assert.equal(await page.evaluate(()=>window.__calls.find(c=>c.cmd==='setup_save_mapping').args.bindings['Buttons/A']),'`Button Y`');
await page.locator('#next').click();
await page.getByRole('heading',{name:'Which button textures?',exact:true}).waitFor();
await page.locator('#back').click();
await page.getByRole('heading',{name:'Your controller',exact:true}).waitFor();
await page.getByRole('combobox',{name:'Your controller',exact:true}).selectOption('0');
await page.locator('#next').click();
await page.getByRole('heading',{name:'Which button textures?',exact:true}).waitFor();
assert.ok(await page.evaluate(()=>window.__calls.find(c=>c.cmd==='onboarding_choose_controller').args.expectedDevice),'selection carries scanned device identity');
await page.goto(url+'/onboarding.html?stage=completion&missing=1');
await page.getByRole('button',{name:'Finish',exact:true}).waitFor();
assert.equal(await page.getByRole('button',{name:'Finish',exact:true}).isDisabled(),true,'setup cannot finish without game and emulator');
await page.setViewportSize({width:800,height:600});
assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'setup horizontal overflow');
await page.setViewportSize({width:1280,height:800});
await page.goto(url+'/recovery.html');await page.getByRole('button',{name:'Choose manifest & verify'}).waitFor();
await page.screenshot({path:path.join(out,'reborn-launcher-recovery.png')});
await page.goto(url+'/index.html');await page.getByRole('button',{name:'Play now'}).waitFor();
// Empty installation must offer a useful next action, without inventing readiness.
await page.evaluate(()=>{window.__fixture.launch_check={ready:false,problem:'No game image was found.'};window.__fixture.onboarding_progress={complete:false,summary:{}};document.dispatchEvent(new Event('refresh-devices'));});
await page.waitForURL('**/onboarding.html');
// Controller navigation across real pages, using sampled standard-map input.
async function navPad(held = null) {
 await page.evaluate(button=>{
  window.__navPad={id:'Xbox navigation test',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:17},(_,i)=>({pressed:i===button,value:i===button?1:0}))};
  Object.defineProperty(navigator,'getGamepads',{configurable:true,value:()=>[window.__navPad]});
 },held);
 await page.waitForTimeout(80);
}
async function tap(button) {
 await page.evaluate(i=>window.__navPad.buttons[i]={pressed:true,value:1},button);
 await page.waitForTimeout(50);
 await page.evaluate(i=>{if(window.__navPad)window.__navPad.buttons[i]={pressed:false,value:0};},button);
 await page.waitForTimeout(50);
}
await page.goto(url+'/controls.html');await page.getByRole('tab').first().waitFor();
await navPad(0);
assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c.cmd==='controls_set').length),0);
await page.evaluate(()=>window.__navPad.buttons[0]={pressed:false,value:0});
await page.waitForTimeout(60);
assert.equal(await page.evaluate(()=>document.body.dataset.controllerTesting),undefined,'Controllers remains navigable on entry');
// Bumper navigation causes a real document change. Release is irrelevant after navigation.
await page.evaluate(()=>window.__navPad.buttons[5]={pressed:true,value:1});
await page.waitForURL('**/online.html');await page.goto(url+'/settings.html');await page.getByRole('button',{name:'Files & storage',exact:true}).waitFor();await page.getByRole('button',{name:'Files & storage',exact:true}).evaluate(n=>n.click());await page.getByLabel('Game image',{exact:true}).waitFor();
await navPad();
assert.equal(await page.evaluate(()=>document.body.classList.contains('gamepad-input')),true,'controller mode survives page changes');
await page.evaluate(()=>document.getElementById('main').scrollTop=0);
await page.evaluate(()=>window.__navPad.axes[3]=.9);await page.waitForTimeout(100);
await page.evaluate(()=>window.__navPad.axes[3]=0);
assert.ok(await page.evaluate(()=>document.getElementById('main').scrollTop)>0,'right stick scrolls long pages');
const scrolling=await page.evaluate(async()=>{
 const main=document.getElementById('main');
 document.dispatchEvent(new WheelEvent('wheel'));main.scrollTop=0;window.__navPad.axes[3]=.7;
 const frames=[];
 for(let i=0;i<18;i++){await new Promise(requestAnimationFrame);frames.push(main.scrollTop);}
 window.__navPad.axes[3]=0;document.dispatchEvent(new WheelEvent('wheel'));
 const stopped=main.scrollTop;await new Promise(r=>setTimeout(r,100));
 return {frames,stopped,after:main.scrollTop};
});
assert.ok(scrolling.frames.at(-1)>0,'gentle stick input scrolls');
assert.ok(scrolling.frames.every((v,i)=>!i||v>=scrolling.frames[i-1]),'scrolling progresses without reversing');
assert.ok(scrolling.frames.every((v,i)=>!i||v-scrolling.frames[i-1]<70),'scrolling has no large frame jumps');
assert.equal(scrolling.after,scrolling.stopped,'manual scroll input cancels remaining animated travel');
await page.goto(url+'/index.html');await page.getByRole('button',{name:'Play now'}).waitFor();
await page.getByRole('button',{name:'Game details',exact:true}).click();
await navPad();
await page.getByRole('dialog').getByRole('link',{name:/Graphics All-Out/}).focus();await tap(1);
assert.equal(await page.locator('#game-details').evaluate(n=>n.open),false,'Controller Back closes game details');
assert.equal(await page.evaluate(()=>document.activeElement?.id),'game-details-open','Back returns focus to the details opener');
const sceneBefore=await page.locator('.hero-art').boundingBox();
await page.getByRole('button',{name:'Game details',exact:true}).click();
await page.getByText('Files & launch details',{exact:true}).click();
assert.deepEqual(await page.locator('.hero-art').boundingBox(),sceneBefore,'opening files does not move the artwork');
await page.screenshot({path:path.join(out,'reborn-play-details.png')});
await page.keyboard.press('Escape');
assert.equal(await page.evaluate(()=>document.activeElement?.id),'game-details-open','Escape restores focus');
await page.evaluate(()=>window.__originalScene=document.querySelector('.hero-art'));
await page.getByRole('button',{name:'Play now'}).click();
assert.equal(await page.evaluate(()=>window.__originalScene.isConnected),true,'launch preserves the artwork element');
await page.evaluate(()=>window.__finishLaunch({started:false,message:'The emulator could not start.'}));
await page.getByRole('alert').waitFor();
assert.deepEqual(await page.locator('.hero-art').boundingBox(),sceneBefore,'launch failure does not resize the artwork');
await page.screenshot({path:path.join(out,'reborn-play-error.png')});
await page.setViewportSize({width:800,height:600});
const footerBefore=await page.locator('.input-footer').boundingBox();
await page.evaluate(()=>document.body.classList.toggle('gamepad-input'));
assert.deepEqual(await page.locator('.input-footer').boundingBox(),footerBefore,'input mode does not change viewport height');
await page.emulateMedia({reducedMotion:'reduce'});
assert.equal(await page.locator('.play').evaluate(n=>getComputedStyle(n).transitionDuration),'0s');
await page.emulateMedia({reducedMotion:'no-preference'});
// Updates stay in a shared dock across pages; manual mode opens a review first.
await page.addInitScript(()=>{
 if(!new URL(location.href).searchParams.has('notice-test'))return;
 sessionStorage.removeItem('updates:last-auto-check');sessionStorage.removeItem('updates:latest');
 window.__fixture.update_check={state:'available',current:'3.0.0',latest:'3.0.1'};
 window.__fixture.update_activity={downloading:false,ready:null};
 window.__fixture.content_status={phase:'available',release:'content-2',download_bytes:4194304,note:'Content update ready.'};
});
await page.setViewportSize({width:800,height:560});
await page.goto(url+'/index.html?notice-test');
await page.waitForFunction(()=>document.querySelectorAll('#update-center .update-card').length===2);
await page.getByRole('button',{name:'Remind me later',exact:true}).click();
await page.getByRole('button',{name:'Remind me later',exact:true}).waitFor();
await page.getByRole('button',{name:'Remind me later',exact:true}).click();
const notices=await page.locator('#update-center .update-card').evaluateAll(nodes=>nodes.map(n=>{const r=n.getBoundingClientRect();return {top:r.top,bottom:r.bottom};}));
assert.ok(notices[0].bottom<=notices[1].top,'update cards do not overlap');
assert.ok(notices[1].bottom<=560,'update cards stay inside the window');
await page.screenshot({path:path.join(out,'reborn-play-updates-800.png')});
assert.equal(await page.evaluate(()=>window.__calls.some(c=>['update_download','content_fetch','content_promote'].includes(c.cmd))),false,'postponed updates do not download or install');
await page.getByRole('button',{name:'View update',exact:true}).first().click();
await page.getByRole('dialog',{name:'Launcher update',exact:true}).waitFor();
await page.getByRole('button',{name:'Remind me later',exact:true}).click();
await page.goto(url+'/index.html?missing');await page.getByRole('link',{name:'Set up your game',exact:true}).waitFor();
assert.equal(await page.locator('#play-now').count(),0,'missing game offers setup instead of launch');
await page.screenshot({path:path.join(out,'reborn-play-setup-800.png')});
await page.setViewportSize({width:1280,height:800});
await page.goto(url+'/controls.html');
await page.getByRole('button',{name:'Test controller',exact:true}).waitFor();
const families = await page.evaluate(() => ['Xbox One','DualSense (054c:0ce6)','DualShock 4','GameCube','Unknown USB device'].map(id => window.createControllerPreview(id).dataset.family));
assert.deepEqual(families,['xbox','dualsense','playstation','gamecube','generic']);
await page.goto(url+'/onboarding.html?stage=migration_offer');
await page.getByRole('radio',{name:/Keep my saves/}).check();
await page.locator('#next').click();
await page.getByRole('button',{name:'Copy it over'}).waitFor();
await page.screenshot({path:path.join(out,'reborn-save-import.png')});
await page.getByRole('button',{name:'Copy it over'}).click();
await page.getByRole('heading',{name:'Choose your graphics setting',exact:true}).waitFor();
assert.equal(await page.evaluate(()=>window.__calls.find(c=>c.cmd==='migration_apply').args.savesOnly),true);
assert.equal(errors.length,0,errors.join('\n'));
console.log('PASS: launcher flows, navigation, onboarding, saves, settings, and motion stability.');
await browser.close();server.close();

await import('./review-players.mjs');

await import('./review-finish.mjs');
