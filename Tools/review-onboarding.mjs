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

const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
const shot=async name=>{await page.evaluate(()=>document.fonts.ready);await page.waitForTimeout(250);await page.screenshot({path:path.join(out,name+'.png')});};
await page.goto(url+'/onboarding.html?stage=welcome');await page.getByRole('radio',{name:/Start fresh/}).check();await page.locator('#next').click();
await page.evaluate(()=>window.__delayCommand='setup_import_game');
await page.getByRole('button',{name:'Browse for NFL Street 2',exact:true}).click();
await page.waitForFunction(()=>window.__releaseDelayed);
await page.evaluate(()=>window.__events['game-import-progress']({payload:{stage:'Applying patch...',percent:72}}));
assert.equal(await page.locator('progress').getAttribute('value'),'72');assert.equal(await page.locator('#next').isDisabled(),true);
await shot('onboarding-patcher-progress');
await page.evaluate(()=>window.__releaseDelayed());
await page.getByText('Patched and verified. Continue to your controller.',{exact:true}).waitFor();
assert.equal(await page.locator('.patch-complete').count(),1);assert.equal(await page.locator('progress').getAttribute('value'),'100');assert.equal(await page.locator('#next').isEnabled(),true);
await shot('onboarding-patcher-complete');
await page.goto(url+'/onboarding.html?stage=game_verification');
await page.getByText('INSTALLATION FOUND',{exact:true}).waitFor();
assert.equal(await page.locator('.import-overrides').getAttribute('open'),null);
assert.equal(await page.locator('#next').isEnabled(),true);
await page.setViewportSize({width:818,height:661});await shot('migration-detected-redesign');
assert.ok(await page.evaluate(()=>document.querySelector('main').scrollHeight<=document.querySelector('main').clientHeight),'Detected install fits without scrolling');
await page.getByText('Change detected files',{exact:true}).click();
await page.getByRole('button',{name:'Browse for a game file',exact:true}).click();
assert.match(await page.locator('.detected-items').innerText(),/NSR.iso/);
await shot('migration-overrides-redesign');await page.setViewportSize({width:1280,height:800});await page.evaluate(()=>window.__delayCommand='setup_migrate_install');await page.getByRole('button',{name:'Import and continue',exact:true}).click();await page.waitForFunction(()=>window.__releaseDelayed);
await page.evaluate(()=>window.__events['install-import-progress']({payload:{stage:'Copying saves and settings...',percent:null,bytes:180*1048576}}));await shot('migration-copying');assert.equal(await page.locator('.patch-card progress').getAttribute('value'),null);
await page.evaluate(()=>window.__events['install-import-progress']({payload:{stage:'Verifying your game...',percent:82,bytes:0}}));assert.equal(await page.locator('.patch-card progress').getAttribute('value'),'82');await shot('migration-verifying');await page.evaluate(()=>window.__releaseDelayed());await page.getByRole('heading',{name:'Your controller',exact:true}).waitFor();
await page.goto(url+'/onboarding.html?stage=controller_setup');
await page.getByRole('combobox',{name:'Your controller',exact:true}).selectOption('0');
await page.waitForFunction(()=>document.querySelector('.vendor-xbox-one')?.getBoundingClientRect().width>100);
await page.evaluate(()=>{window.__pad={id:'Xbox One (XInput STANDARD GAMEPAD)',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:17},()=>({pressed:false,value:0}))};Object.defineProperty(navigator,'getGamepads',{value:()=>[window.__pad],configurable:true});});
await page.getByText('Connected · live input ready',{exact:true}).waitFor();
await shot('onboarding-controller-polished');
assert.equal(await page.locator('.compact-mappings button').count(),6);
assert.equal(await page.evaluate(()=>document.body.dataset.controllerTesting),'setup');
await page.evaluate(()=>window.__pad.buttons[0]={pressed:true,value:1});
await page.waitForFunction(()=>document.querySelector('.controller-art [data-button="0"]')?.dataset.pressed==='true');
assert.equal(await page.getByRole('heading',{name:'Your controller',exact:true}).count(),1);
await page.evaluate(()=>window.__pad.buttons[0]={pressed:false,value:0});
await page.keyboard.press('Escape');assert.equal(await page.evaluate(()=>document.body.dataset.controllerTesting),undefined);
await page.getByRole('button',{name:'Test controller',exact:true}).click();
// First run keeps the bindings behind "Customize controls" (the standard
// layout is right for nearly everyone); a rebind starts by opening it.
await page.getByText('Customize controls',{exact:true}).click();
await page.getByRole('button',{name:'Assign A',exact:true}).click();
await page.waitForTimeout(80);await page.evaluate(()=>window.__pad.buttons[3]={pressed:true,value:1});
await page.getByText('A saved.',{exact:true}).waitFor();
assert.equal(await page.evaluate(()=>window.__calls.find(c=>c.cmd==='setup_save_mapping').args.bindings['Buttons/A']),'`Button Y`');
await page.evaluate(()=>window.__pad.buttons[3]={pressed:false,value:0});
await page.evaluate(()=>window.__reject='setup_save_mapping');await page.getByRole('button',{name:'Assign A',exact:true}).click();
// Typing a Dolphin input name is the fallback, folded away behind one line.
await page.getByText('Type a Dolphin input name instead',{exact:true}).click();
await page.getByRole('textbox',{name:'A binding',exact:true}).fill('`Button X`');await page.getByRole('button',{name:'Use binding',exact:true}).click();
await page.getByText('Test failure: setup_save_mapping',{exact:true}).waitFor();assert.match(await page.getByRole('button',{name:'Assign A',exact:true}).innerText(),/Button Y/);
await page.evaluate(()=>window.__reject=null);
await page.getByRole('button',{name:'Sticks',exact:true}).click();assert.equal(await page.locator('.compact-mappings button').count(),8);
const stickRows=await page.locator('.compact-mappings button').allTextContents();assert.match(stickRows[0],/Left stick up/i);assert.match(stickRows[1],/Right stick up/i);await shot('onboarding-sticks');
await page.getByRole('button',{name:'Buttons',exact:true}).click();
// The small-window measurement is of what a first-run player sees: the
// standard layout with the bindings folded away, not the opened grid.
await page.getByText('Customize controls',{exact:true}).click();
// The size first run actually runs at (setup_window_lock holds 1280x800):
// no scrollbar on the controller step, folded or opened.
await page.setViewportSize({width:1280,height:800});await page.waitForTimeout(200);
const lockedSize=await page.evaluate(()=>{const main=document.querySelector('main');const parts=el=>Array.from(el.children).map(c=>`${c.tagName.toLowerCase()}${c.id?'#'+c.id:''}${c.className?'.'+String(c.className).split(' ')[0]:''}=${c.offsetHeight}`);return {scroll:main.scrollHeight,height:main.clientHeight,screenParts:parts(document.getElementById('screen'))};});
console.log('controller layout at 1280x800 (folded)',JSON.stringify(lockedSize));assert.ok(lockedSize.scroll<=lockedSize.height,'the controller step must not scroll at 1280x800');
await page.getByText('Customize controls',{exact:true}).click();await page.waitForTimeout(200);
const openedSize=await page.evaluate(()=>({scroll:document.querySelector('main').scrollHeight,height:document.querySelector('main').clientHeight}));
console.log('controller layout at 1280x800 (opened)',openedSize);assert.ok(openedSize.scroll<=openedSize.height,'the controller step must not scroll at 1280x800 with the bindings open');
// The stage clips its overflow, so "no page scroll" is not enough: the Swap
// and Restore buttons under the opened grid must be reachable, inside the
// stage's box or inside a fold that scrolls on its own.
const opened=await page.evaluate(()=>{const stage=document.querySelector('.setup-controller').getBoundingClientRect();const fold=document.querySelector('.customize-controls');const f=fold.getBoundingClientRect();const buttons=document.querySelector('.customize-controls .mapping-buttons').getBoundingClientRect();return {stageBottom:stage.bottom,foldBottom:f.bottom,buttonsBottom:buttons.bottom,foldScrolls:fold.scrollHeight>fold.clientHeight+1,foldOverflow:getComputedStyle(fold).overflowY};});
console.log('opened bindings',opened);
assert.ok(opened.foldBottom<=opened.stageBottom+1,'the opened fold must end inside the stage');
assert.ok(opened.buttonsBottom<=opened.stageBottom+1||(opened.foldScrolls&&['auto','scroll'].includes(opened.foldOverflow)),'Swap and Restore must be inside the stage or inside a fold that scrolls');
await shot('onboarding-controller-1280');
await page.getByText('Customize controls',{exact:true}).click();
await page.setViewportSize({width:800,height:600});await shot('onboarding-controller-800');
const layout=await page.evaluate(()=>({scroll:document.querySelector('main').scrollHeight,height:document.querySelector('main').clientHeight,width:document.documentElement.scrollWidth,inner:innerWidth}));
console.log('controller layout',layout);assert.ok(layout.width<=layout.inner);
// The launcher holds its window at 1280x800 for the whole of first run
// (setup_window_lock), so 800x600 is a stress test, not a real case. The
// page's own bottom bar overlaps main by 8px at this height whatever the
// step shows (the same 8px with the bindings folded or open, the art at
// 165px or 185px); everything the step draws is inside the card.
assert.ok(layout.scroll<=layout.height+8,'controller workspace needs no scrolling at 800x600 beyond the shell bar overlap');
await page.setViewportSize({width:1280,height:800});
await page.goto(url+'/onboarding.html?stage=preparation');
await page.getByRole('button',{name:/Edit Graphics/}).click();
await page.locator('input[value=performance]').check();await page.getByRole('button',{name:'Save and return',exact:true}).click();
await page.getByRole('heading',{name:'Review your settings',exact:true}).waitFor();
assert.match(await page.getByRole('button',{name:/Edit Graphics/}).innerText(),/Performance/);
await page.getByRole('button',{name:/Edit Button textures/}).click();
await page.locator('input[value=playstation]').check();await shot('onboarding-textures-polished');
await page.getByRole('button',{name:'Save and return',exact:true}).click();
await page.getByRole('heading',{name:'Review your settings',exact:true}).waitFor();
await shot('onboarding-recap-polished');
await page.getByRole('button',{name:/Edit Game:/}).click();await page.getByRole('button',{name:'Save and return',exact:true}).click();
await page.getByRole('heading',{name:'Review your settings',exact:true}).waitFor();
assert.equal(await page.getByRole('combobox',{name:'Resolution',exact:true}).count(),0);
await page.evaluate(()=>window.__fixture.previousGraphics={label:'Previous settings',resolution:2,msaa:4,ssaa:false});
await page.getByRole('button',{name:/Edit Graphics/}).click();await page.locator('input[value=previous]').check();await page.setViewportSize({width:800,height:600});await shot('onboarding-four-presets-800');await page.setViewportSize({width:1280,height:800});await shot('onboarding-four-presets');await page.getByRole('button',{name:'Save and return',exact:true}).click();await page.getByRole('heading',{name:'Review your settings',exact:true}).waitFor();
await page.locator('#next').click();await page.getByRole('heading',{name:"You're all set",exact:true}).waitFor();
await page.getByText("Your settings are applied. You're ready to hit the streets!",{exact:true}).waitFor();await shot('onboarding-complete-polished');
await page.locator('#next').click();await page.waitForURL('**/index.html');
await page.goto(url+'/onboarding.html?stage=updates');
assert.equal(await page.locator('.recommendation-badge').innerText(),'Recommended');
assert.equal(await page.locator('.recommended input').inputValue(),'yes');
await page.setViewportSize({width:818,height:661});await shot('updates-consistent-badge');
await page.goto(url+'/onboarding.html?stage=system_check');
assert.equal(await page.getByText('1080p (3×) · Anti-aliasing off',{exact:true}).count(),0);
await shot('graphics-cleaned');
await page.goto(url+'/onboarding.html?stage=game_verification');
await page.goto(url+'/onboarding.html?stage=game_verification&noinstall');
await page.getByText('FIND YOUR INSTALLATION',{exact:true}).waitFor();
assert.equal(await page.locator('.import-overrides').getAttribute('open'),'');
assert.equal(await page.locator('#next').isDisabled(),true);
await page.getByText('No existing saves found',{exact:true}).waitFor();await shot('migration-not-found');
await page.evaluate(()=>window.__reject='pick_path');
await page.getByRole('button',{name:'Choose installation folder',exact:true}).click();
await page.getByText('Test failure: pick_path',{exact:true}).waitFor();
assert.equal(await page.getByRole('button',{name:'Choose installation folder',exact:true}).isEnabled(),true);
await page.evaluate(async()=>{window.__reject=null;window.__fixture.setup_info={mode:'migrate',root:'C:/Reborn',games:['C:/Reborn/Games/NSR.iso'],memory_cards:[],settings_found:true};});
await page.getByRole('button',{name:'Scan again',exact:true}).click();
await page.getByText('INSTALLATION FOUND',{exact:true}).waitFor();
assert.equal(await page.locator('#next').isEnabled(),true);
await page.goto(url+'/onboarding.html?stage=game_verification');
await page.getByText('Release 2.1',{exact:true}).waitFor();
await page.evaluate(()=>{window.__reject='setup_migrate_install';window.__rejectMessage='Close Reborn Dolphin before applying settings, then try again.';});
await page.locator('#next').click();
await page.getByText('Close Reborn Dolphin to continue',{exact:true}).waitFor();
assert.equal(await page.locator('.patch-card').isVisible(),false);
assert.equal(await page.locator('.detected-install').isVisible(),true);
assert.equal(await page.locator('#next').isEnabled(),true);
await page.locator('#next').click();assert.equal(await page.locator('.step-error').count(),1);
await shot('migration-dolphin-open');
await page.evaluate(()=>window.__reject=null);await page.locator('#next').click();
await page.getByRole('heading',{name:'Your controller',exact:true}).waitFor();
assert.equal(errors.length,0,errors.join('\n'));
console.log('PASS: live artwork, automatic test, Escape navigation, physical capture autosave, compact mapping groups, recap edits return directly, saving and Finish to Play.');
await browser.close();server.close();
