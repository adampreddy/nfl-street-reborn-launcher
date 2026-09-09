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
fixture.mod_packages = {packages_root:'C:/Reborn/mods',packages:[{id:'football-red',version:'1.0',name:'Red football',active:true,files:1,bytes:1048576},{id:'football-black',version:'1.0',name:'Black football',active:false,files:1,bytes:1048576},{id:'soundtrack-classic',version:'1.0',name:'Classic soundtrack',active:false,files:12,bytes:18000000},{id:'textures',version:'2.1',name:'Reborn textures',active:true,files:54,bytes:18000000}],activation:[{id:'football-red',version:'1.0'},{id:'textures',version:'2.1'}],unreadable:[],exclusive:[{members:['football-red','football-black'],computed:true,example_path:'football.png'}],managed_by_controls:[]};
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
  if(cmd==='dolphin_preferences') return {editable:true,user_dir:'C:/Reborn/User',values:{audio_backend:'Cubeb',volume:'100',muted:'False',audio_stretch:'False',audio_latency:'20',fullscreen:'True',always_on_top:'False',vsync:'False',show_fps:'False',show_ping:'False',borderless:'False',confirm_stop:'True',pause_unfocused:'False',osd_messages:'True',slot_a:'8',slot_b:'255',card_a:'',card_b:'',gci_a:'',gci_b:'',...JSON.parse(localStorage.getItem('qa:dolphin')||'{}')}};
  if(cmd==='dolphin_preference_set'){const current=JSON.parse(localStorage.getItem('qa:dolphin')||'{}');current[args.key]=args.value;localStorage.setItem('qa:dolphin',JSON.stringify(current));return;}
  if(cmd==='open_runtime'||cmd==='dolphin_saves_folder')return {started:true};
  if(cmd==='update_policy_set'){data.update_policy={automatic:args.automatic,source:'settings'};return data.update_policy;}
  if(cmd==='update_check')return {state:'available',latest:'3.0.1',note:'A new version is available.',notes:'Improved controller support.'};
  if(cmd==='update_download')return {path:'C:/Reborn/update.exe',bytes:38000000};
  if(cmd==='netplay_open'){data.session_active=true;data.netplay_status={phase:'lobby',active:true,host:args.mode==='host',connection:args.connection,port:args.port,code:args.connection==='traversal'?'ABC12345':'',can_start:args.mode==='host',players:[{id:1,name:'Player One',status:'OK',ping:'0 ms'}],ports:[1,0,0,0]};return {started:true};}
  if(cmd==='netplay_status')return data.netplay_status||{phase:'idle',active:false};
  if(cmd==='netplay_action'){if(args.action==='leave'){data.session_active=false;data.netplay_status={phase:'closed',active:false};}if(args.action==='start'){data.netplay_status.phase='running';data.netplay_status.can_start=false;}if(args.action==='ports')data.netplay_status.ports=args.ports;return;}
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

const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));
try{
 await page.goto(url+'/online.html');
 await page.getByLabel('Player name',{exact:true}).fill('Player One');await page.getByLabel('Host code',{exact:true}).fill('ABC12345');
 await page.getByRole('button',{name:'Join game',exact:true}).click();await page.getByRole('heading',{name:'You’re in.',exact:true}).waitFor();
 let args=await page.evaluate(()=>window.__calls.find(c=>c.cmd==='netplay_open').args);assert.equal(args.mode,'join');assert.equal(args.connection,'traversal');assert.equal(args.address,'ABC12345');
 assert.equal(await page.getByRole('button',{name:'Start game',exact:true}).count(),0);
 await page.getByRole('button',{name:'Leave room',exact:true}).click();await page.getByRole('button',{name:'Join game',exact:true}).waitFor();
 await page.getByRole('button',{name:'Host a game',exact:true}).click();await page.getByLabel('Connection',{exact:true}).selectOption('direct');await page.getByLabel('Port',{exact:true}).fill('2626');await page.getByRole('button',{name:'Create room',exact:true}).click();await page.getByRole('heading',{name:'Ready when you are.',exact:true}).waitFor();
 assert.equal(await page.locator('.netplay-slots select').count(),4);
 await page.getByLabel('Player 2 assignment',{exact:true}).selectOption('1');await page.waitForFunction(()=>window.__fixture.netplay_status.ports[1]===1);
 await page.screenshot({path:path.join(out,'finish-online.png')});
 await page.getByRole('button',{name:'Start game',exact:true}).click();await page.getByRole('heading',{name:'Game in progress',exact:true}).waitFor();
 await page.getByRole('button',{name:'End session',exact:true}).click();await page.getByRole('button',{name:'Create room',exact:true}).waitFor();
 await page.evaluate(()=>window.__reject='netplay_open');await page.getByRole('button',{name:'Create room',exact:true}).click();await page.getByText('Test failure: netplay_open',{exact:true}).waitFor();
 assert.equal(await page.getByRole('button',{name:'Create room',exact:true}).isEnabled(),true);
 assert.equal(await page.getByRole('textbox',{name:'Player name'}).inputValue(),'Player One');

 assert.deepEqual(errors,[]);console.log('PASS: join/host lobby, four player assignments, start/leave, and error recovery.');
}finally{await browser.close();server.close();}
