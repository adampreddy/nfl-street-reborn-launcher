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
 update_check: {state:'current',current:'3.0.0'},
 update_activity: {downloading:false,ready:null},
 content_status: {phase:'up_to_date',note:'Current'},
 content_check: {phase:'up_to_date',note:'Current'},
 onboarding_progress: {complete:true,next:{kind:'already_complete'},plan:[],set_aside:[],summary:{graphics:'balanced'}},
});
fixture.mod_packages = {packages_root:'C:/Reborn/mods',packages:[{id:'football-red',version:'1.0',name:'Red football',active:true,files:1,bytes:1048576},{id:'football-black',version:'1.0',name:'Black football',active:false,files:1,bytes:1048576},{id:'textures',version:'2.1',name:'Reborn textures',active:true,files:54,bytes:18000000}],activation:[{id:'football-red',version:'1.0'},{id:'textures',version:'2.1'}],unreadable:[],exclusive:[{members:['football-red','football-black'],computed:true,example_path:'football.png'}],managed_by_controls:[]};
fixture.controls_options=[{slug:'xbox',label:'Xbox',suggested:true,describes:'A, B, X and Y prompts.',pack:'Xbox Controls'},{slug:'playstation',label:'PlayStation',suggested:false,describes:'Cross, circle, square and triangle prompts.',pack:'PS Controls'},{slug:'gamecube',label:'GameCube',describes:'Original game prompts.',pack:null}];
fixture.devices={complete:true,pads:Array.from({length:4},(_,id)=>({device_string:`XInput/${id}/Gamepad`,name:'Gamepad',backend:'XInput',id}))};
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
 window.__calls=[]; window.__fixture=data;window.__events={};
 const stage=new URL(location.href).searchParams.get('stage');
 if(stage) {
  const stages=['welcome','game_verification','controller_setup','controls_profile','system_check','updates','preparation','completion'];
  data.onboarding_progress={complete:false,next:{kind:'show',stage,position:stages.indexOf(stage)+1,total:stages.length},plan:stages.map(s=>({stage:s,satisfied:stages.indexOf(s)<stages.indexOf(stage)})),set_aside:[],summary:{},preparation_remaining:[],tasks_done:[]};
 }
 if(new URL(location.href).searchParams.has('noinstall')) data.setup_info={mode:'migrate',root:null,games:[],memory_cards:[]};
 if(new URL(location.href).searchParams.has('missing')) data.launch_check={ready:false,problem:'No game image was found.'};
 window.__TAURI__={event:{listen:async(name,fn)=>{window.__events[name]=fn;return ()=>delete window.__events[name];}},core:{invoke:async(cmd,args={})=>{
  window.__calls.push({cmd,args});
  if(window.__delayCommand===cmd) await new Promise(resolve=>{window.__releaseDelayed=resolve;});
  if(window.__reject===cmd) throw new Error(window.__rejectMessage||('Test failure: '+cmd));
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
  if(cmd==='onboarding_acknowledge') {data.onboarding_progress.next.stage='game_verification';return {outcome:'recorded'};}
  if(cmd==='onboarding_choose_controls') {data.onboarding_progress.summary.controls_profile=args.profile;data.onboarding_progress.next.stage='system_check';return {outcome:'recorded'};}
  if(cmd==='onboarding_choose_graphics') {data.onboarding_progress.summary.graphics=args.preset;data.onboarding_progress.next.stage='updates';return {outcome:'recorded'};}
  if(cmd==='onboarding_choose_updates') {data.onboarding_progress.next.stage='preparation';return {outcome:'recorded'};}
  if(cmd==='onboarding_prepared') {data.onboarding_progress.next.stage='completion';return {outcome:'recorded'};}
  if(cmd==='setup_graphics_options') return {current:data.graphicsOptions||null,previous:data.previousGraphics||null};
  if(cmd==='setup_save_graphics_options') {data.graphicsOptions=args.mode==='preset'?null:{label:args.mode==='previous'?'Previous settings':'Custom',resolution:args.resolution,msaa:args.msaa,ssaa:args.ssaa};return;}
  if(cmd==='setup_migrate_install'){data.onboarding_progress.next.stage='controller_setup';return {imported:true};}
  if(cmd==='setup_import_existing_game') return 'C:/Reborn/library/imported/test/NSR.iso';
  if(cmd==='setup_hardware_info') return {description:'NVIDIA GeForce RTX 4070 · 32 GB RAM',recommended:'quality'};
  if(cmd==='controller_slots') return {slots:JSON.parse(localStorage.getItem('qa:slots')||'null')};
  if(cmd==='controller_assign_player'){const slots=JSON.parse(localStorage.getItem('qa:slots')||'null')||data.devices.pads.map(p=>p.device_string);for(let i=0;i<4;i++)if(slots[i]===args.device)slots[i]=null;slots[args.player-1]=args.device;localStorage.setItem('qa:slots',JSON.stringify(slots));return {slots};}
  if(cmd==='setup_controller_mapping') return {custom:false,rows:[...['A','B','X','Y','Z','Start'].map(x=>({key:'Buttons/'+x,label:x,binding:JSON.parse(localStorage.getItem('qa:map:'+args.device)||'{}')['Buttons/'+x]||'`Button '+x+'`'})),...['Up','Down','Left','Right'].flatMap(x=>['Main Stick','C-Stick','D-Pad'].map(g=>({key:g+'/'+x,label:g+' '+x,binding:'`'+x+'`'}))),...['L','R'].map(x=>({key:'Triggers/'+x,label:x,binding:'`Trigger '+x+'`'}))]};
  if(cmd==='remap_start'||cmd==='remap_stop') return {};
  if(cmd==='remap_detect') return new Promise(resolve=>{window.__completeRemap=resolve;window.__remapDevice=args.device;});
  if(cmd==='setup_save_mapping'){const key='qa:map:'+args.device;localStorage.setItem(key,JSON.stringify(args.reset?{}:{...JSON.parse(localStorage.getItem(key)||'{}'),...args.bindings}));return;}
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

const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
try {
 await page.goto(url+'/controls.html');
 await page.getByRole('button',{name:'Assign A',exact:true}).waitFor();
 assert.equal(await page.getByRole('tab').count(),4);
 assert.equal(await page.locator('.mapping-group').count(),5);
 await page.getByRole('tab',{name:/Player 3/}).click();
 await page.getByRole('button',{name:'Assign A',exact:true}).waitFor();
 assert.equal(await page.getByRole('combobox',{name:'Controller for Player 3'}).inputValue(),'XInput/2/Gamepad');
 await page.evaluate(()=>{
  window.__pads=Array.from({length:4},(_,index)=>({id:'Xbox One (XInput STANDARD GAMEPAD)',index,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:17},()=>({pressed:false,value:0}))}));
  Object.defineProperty(navigator,'getGamepads',{value:()=>window.__pads,configurable:true});
 });
 assert.equal(await page.evaluate(()=>document.body.dataset.controllerTesting),undefined,'Entering Controllers keeps navigation on');
 await page.getByRole('button',{name:'Test controller',exact:true}).click();
 await page.getByRole('combobox',{name:'Controller to test'}).selectOption('2');
 assert.equal(await page.evaluate(()=>document.body.dataset.controllerTesting),'setup');
 await page.getByRole('button',{name:'Finish test',exact:true}).focus();
 const focusBefore=await page.evaluate(()=>document.activeElement.textContent);
 await page.evaluate(()=>document.fonts.ready);
 await page.waitForTimeout(200);
 const scrollBefore=await page.evaluate(()=>document.getElementById('main').scrollTop);
 await page.waitForTimeout(100);
 await page.evaluate(()=>window.__pads.forEach(p=>{p.buttons.forEach((b,i)=>{b.pressed=i!==8&&i!==9;b.value=b.pressed?1:0;});p.axes=[1,1,1,1];}));
 await page.waitForTimeout(300);
 assert.ok(page.url().endsWith('/controls.html'),'buttons and bumpers cannot navigate during testing');
 assert.equal(await page.evaluate(()=>document.body.dataset.controllerTesting),'setup','View + Menu cannot accidentally exit testing');
 assert.equal(await page.evaluate(()=>document.activeElement.textContent),focusBefore,'D-pad does not move focus');
 assert.equal(await page.evaluate(()=>document.getElementById('main').scrollTop),scrollBefore,'sticks do not scroll during testing');
 await page.evaluate(()=>window.__pads.forEach(p=>{p.buttons.forEach(b=>{b.pressed=false;b.value=0;});p.axes=[0,0,0,0];}));
 await page.getByRole('button',{name:'Finish test',exact:true}).click();
 assert.equal(await page.evaluate(()=>document.body.dataset.controllerTesting),undefined);
 await page.getByRole('button',{name:'Test controller',exact:true}).click();
 await page.getByRole('button',{name:'Assign A',exact:true}).click();
 assert.equal(await page.getByRole('tab').first().isDisabled(),true);
 await page.waitForTimeout(100);
 await page.evaluate(()=>window.__pads[0].buttons[0]={pressed:true,value:1});
 await page.waitForTimeout(100);
 assert.equal(await page.evaluate(()=>window.__calls.filter(c=>c.cmd==='setup_save_mapping').length),0);
 await page.waitForFunction(()=>!!window.__completeRemap);
 assert.equal(await page.evaluate(()=>window.__remapDevice),'XInput/2/Gamepad');
 await page.evaluate(()=>window.__completeRemap({binding:'`Button Y`'}));
 await page.getByText('A saved.',{exact:true}).waitFor();
 assert.match(await page.getByRole('button',{name:'Assign A',exact:true}).innerText(),/Button Y/);
 assert.equal(await page.evaluate(()=>window.__calls.find(c=>c.cmd==='setup_save_mapping').args.device),'XInput/2/Gamepad');
 await page.evaluate(()=>window.__pads.forEach(p=>p.buttons.forEach(b=>{b.pressed=false;b.value=0;})));
 await page.getByRole('button',{name:'Assign Left stick up',exact:true}).click();
 // Typing a Dolphin input name is folded away behind one line; open it.
 await page.getByText('Type a Dolphin input name instead',{exact:true}).click();
 await page.getByRole('textbox',{name:'Left stick up binding'}).fill('`Left Y+`');
 await page.getByRole('button',{name:'Use binding',exact:true}).click();
 assert.match(await page.getByRole('button',{name:'Assign Left stick up',exact:true}).innerText(),/Left Y\+/);
 await page.getByRole('button',{name:'Assign A',exact:true}).click();await page.keyboard.press('Escape');
 assert.equal(await page.getByRole('dialog').count(),0);
 await page.getByRole('combobox',{name:'Controller for Player 3'}).selectOption('XInput/0/Gamepad');
 await page.getByRole('tab',{name:/Player 1.*No controller/}).waitFor();
 await page.reload();await page.getByText('Connect a controller for Player 1',{exact:true}).waitFor();
 const centered=await page.locator('.player-empty').evaluate(n=>{const box=n.getBoundingClientRect();return [...n.children].every(child=>{const r=child.getBoundingClientRect();return Math.abs(r.x+r.width/2-box.x-box.width/2)<1&&getComputedStyle(child).textAlign==='center';});});
 assert.ok(centered,'empty heading and description share the center of the card');
 await page.screenshot({path:path.join(out,'players-empty-centered.png')});
 await page.getByRole('tab',{name:/Player 3/}).click();await page.getByRole('button',{name:'Assign A',exact:true}).waitFor();
 assert.equal(await page.getByRole('combobox',{name:'Controller for Player 3'}).inputValue(),'XInput/0/Gamepad');
 assert.match(await page.getByRole('button',{name:'Assign A',exact:true}).innerText(),/Button A/);
 await page.evaluate(()=>window.__reject='controller_assign_player');
 await page.getByRole('combobox',{name:'Controller for Player 3'}).selectOption('XInput/1/Gamepad');
 await page.getByText('Test failure: controller_assign_player',{exact:true}).waitFor();
 assert.equal(await page.getByRole('combobox',{name:'Controller for Player 3'}).inputValue(),'XInput/0/Gamepad');
 await page.evaluate(()=>{window.__reject=null;window.__fixture.devices.pads=window.__fixture.devices.pads.filter(p=>p.device_string!=='XInput/0/Gamepad');});
 await page.getByRole('button',{name:'Scan again',exact:true}).click();await page.getByText('Controller disconnected',{exact:true}).waitFor();
 await page.evaluate(()=>window.__fixture.devices.pads.push({device_string:'XInput/0/Gamepad'}));
 await page.getByRole('button',{name:'Scan again',exact:true}).click();await page.getByRole('button',{name:'Assign A',exact:true}).waitFor();
 await page.getByRole('combobox',{name:'Controller for Player 3'}).selectOption('XInput/2/Gamepad');
 await page.getByRole('button',{name:'Assign A',exact:true}).waitFor();
 assert.match(await page.getByRole('button',{name:'Assign A',exact:true}).innerText(),/Button Y/);
 await page.getByRole('button',{name:'Restore automatic mapping',exact:true}).click();
 await page.waitForFunction(()=>document.querySelector('[aria-label="Assign A"]')?.textContent.includes('Button A'));
 await page.getByRole('combobox',{name:'Button textures',exact:true}).selectOption('playstation');
 await page.getByText('Button textures saved. They apply to all players.',{exact:true}).waitFor();
 for(const width of [1280,800]){
  await page.setViewportSize({width,height:800});await page.evaluate(()=>document.fonts.ready);await page.waitForTimeout(200);
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'no horizontal overflow');
  await page.evaluate(()=>document.getElementById('main').scrollTop=0);await page.screenshot({path:path.join(out,`players-${width}.png`),fullPage:true});
 }
 assert.deepEqual(errors,[]);console.log('PASS: four players, assignment persistence, reconnection, isolated capture, edits, cancellation, reset, textures, responsive layout.');
} finally {await browser.close();server.close();}
