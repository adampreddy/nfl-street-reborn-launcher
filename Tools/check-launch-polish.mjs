import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui'),out=path.join(root,'target/browser-review');
mkdirSync(out,{recursive:true});
const source=readFileSync(path.join(root,'Tools/check-shell-pages.mjs'),'utf8');
const fixture=vm.runInNewContext(source.slice(source.indexOf('const FULL ='),source.indexOf('// Every field null'))+'\nFULL');
Object.assign(fixture,{
 session_active:false,settings_paths:{runtime:null,game:null,userDir:null,legacyRoot:null},
 onboarding_progress:{complete:true,next:{kind:'already_complete'},summary:{}},
 update_check:{state:'current',current:'3.0.0'},update_activity:{downloading:false,ready:null},update_policy:{automatic:false},
 content_status:{phase:'up_to_date'},content_check:{phase:'up_to_date'},
 devices:{complete:true,pads:Array.from({length:4},(_,id)=>({device_string:`XInput/${id}/Gamepad`,name:'Gamepad',backend:'XInput',id}))},
 controller_slots:{slots:Array.from({length:4},(_,i)=>`XInput/${i}/Gamepad`)},
 stats_games:Array.from({length:11},(_,i)=>({key:'game-'+i,captured_at:`2026-09-${String(i+1).padStart(2,'0')}T19:30:00Z`,source:'dolphin',
  teams:[{name:i===10?'Baltimore Ravens':'Dallas Cowboys',score:28},{name:'Philadelphia Eagles',score:21}],
  team_totals:[{score:28,total_td:4,pass_td:2,rush_td:2,sacks:3,int:1,first_downs:5},{score:21,total_td:3,pass_td:1,rush_td:2,sacks:1,int:2,first_downs:4}],
  players:[{name:'Lamar Jackson',side:0,team:'Baltimore Ravens',stats:{pass_td:2,rush_td:2,tackles:3}},{name:'Jalen Hurts',side:1,team:'Philadelphia Eagles',stats:{pass_td:1,rush_td:2,tackles:4}}],sides_verified:true}))
});
fixture.stats_queue.entries=fixture.stats_games.map((game,i)=>({key:game.key,capture_folder:i%2?'online':'offline'}));
const mime={'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const p=path.resolve(ui,'.'+decodeURIComponent(new URL(req.url,'http://test').pathname));if(!p.startsWith(ui+path.sep))throw Error('outside');res.writeHead(200,{'Content-Type':mime[path.extname(p)]||'application/octet-stream'});res.end(readFileSync(p));}catch(_){res.writeHead(404);res.end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const base=`http://127.0.0.1:${server.address().port}`;
const browser=await chromium.launch({channel:'msedge',headless:true});
const context=await browser.newContext({viewport:{width:1280,height:800}});
await context.addInitScript(data=>{
 window.calls=[];window.data=data;window.bindings={};
 window.__TAURI__={event:{listen:async()=>()=>{}},core:{invoke:async(cmd,args={})=>{
  calls.push({cmd,args});
  if(cmd==='dolphin_preferences')return {editable:!sessionStorage.live,live_editable:sessionStorage.live?['volume','muted']:[],values:{volume:'100',muted:'False',fullscreen:'True',audio_backend:'Cubeb',audio_stretch:'False',audio_latency:'20'}};
  if(cmd==='setup_controller_mapping')return {rows:[...['A','B','X','Y','Z','Start'].map(n=>({key:'Buttons/'+n,label:n,binding:bindings['Buttons/'+n]||'`Button '+n+'`'})),...['Main Stick','C-Stick','D-Pad'].flatMap(g=>['Up','Down','Left','Right'].map(d=>({key:g+'/'+d,label:g+' '+d,binding:'`'+d+'`'}))),...['L','R'].map(n=>({key:'Triggers/'+n,label:n,binding:'`Trigger '+n+'`'}))]};
  if(cmd==='setup_save_mapping'){Object.assign(bindings,args.bindings);return;}
  if(cmd==='dolphin_preference_set')return;
  if(cmd==='identity_set_show_offline'){data.identity_status.show_offline=args.show;return;}
  if(cmd==='stats_submit'){data.stats_queue.depth=0;return {note:'Results synced.'};}
  if(cmd==='session_active')return !!sessionStorage.live;
  if(cmd==='ui_notices')return [];
  if(cmd in data)return data[cmd];
  return null;
 }}};
},fixture);
const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));
try {
 // Stats v2 replaced the old searchable history (Tools/check-stats-page.mjs covers the page); what stays here is the
 // account and sync block this checker always drove.
 await page.goto(base+'/stats.html');await page.getByText('Profile & sync ›',{exact:true}).waitFor();
 await page.getByText('Profile & sync ›',{exact:true}).click();await page.getByRole('checkbox',{name:'Show my offline games on my profile'}).check();
 await page.waitForFunction(()=>calls.some(c=>c.cmd==='identity_set_show_offline'&&c.args.show));
 await page.getByRole('button',{name:'Send results now',exact:true}).click();await page.waitForFunction(()=>calls.some(c=>c.cmd==='stats_submit'));
 await page.setViewportSize({width:1280,height:800});
 await page.goto(base+'/controls.html');await page.getByRole('button',{name:'Assign B',exact:true}).waitFor();
 assert.equal(await page.evaluate(()=>document.body.dataset.controllerTesting),undefined);
 assert.equal(await page.getByRole('button',{name:'Correct reversed B / X',exact:true}).isVisible(),false);
 await page.evaluate(()=>{window.bindings={'Buttons/B':'`Button X`','Buttons/X':'`Button B`'};});
 await page.getByRole('button',{name:'Scan again'}).click();await page.getByRole('button',{name:'Correct reversed B / X',exact:true}).click();
 await page.getByText('Corrected. B maps to B and X maps to X.',{exact:true}).waitFor();
 assert.equal(await page.getByRole('button',{name:'Correct reversed B / X',exact:true}).isVisible(),false);
 await page.evaluate(()=>{
  sessionStorage.live='true';
  window.pad={id:'Xbox One (XInput STANDARD GAMEPAD)',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:17},()=>({pressed:false,value:0}))};
  Object.defineProperty(navigator,'getGamepads',{configurable:true,value:()=>[pad]});
 });
 await page.getByRole('tab',{name:/Player 1/}).focus();await page.waitForTimeout(150);
 await page.evaluate(()=>pad.buttons[15]={pressed:true,value:1});await page.waitForTimeout(120);await page.evaluate(()=>pad.buttons[15]={pressed:false,value:0});
 assert.match(await page.evaluate(()=>document.activeElement.textContent),/Player 2/,'D-pad navigation works while a game is running');
 await page.getByRole('button',{name:'Test controller',exact:true}).click();assert.equal(await page.evaluate(()=>document.body.dataset.controllerTesting),'setup');
 await page.getByRole('combobox',{name:'Controller to test'}).selectOption('0');
 await page.evaluate(()=>{pad.buttons[8]={pressed:true,value:1};pad.buttons[9]={pressed:true,value:1};});
 await page.waitForFunction(()=>!document.body.dataset.controllerTesting);
 await page.evaluate(()=>{pad.buttons[8]={pressed:false,value:0};pad.buttons[9]={pressed:false,value:0};});
 // A pad user who never picks "Controller to test" can still leave the test: hold B, or View + Menu, on any pad.
 const testOn=()=>page.evaluate(()=>!!document.body.dataset.controllerTesting);
 const pickNone=()=>page.getByRole('combobox',{name:'Controller to test'}).evaluate(s=>{s.value='';s.dispatchEvent(new Event('change',{bubbles:true}));});
 await page.getByRole('button',{name:'Test controller',exact:true}).click();await pickNone();assert.equal(await testOn(),true);
 await page.evaluate(()=>pad.buttons[1]={pressed:true,value:1});await page.waitForTimeout(300);
 assert.equal(await testOn(),true,'a short B press is still tested, not an exit');
 await page.waitForFunction(()=>!document.body.dataset.controllerTesting,null,{timeout:3000});
 await page.evaluate(()=>pad.buttons[1]={pressed:false,value:0});await page.waitForTimeout(150);
 assert.equal(await page.evaluate(()=>location.pathname.endsWith('controls.html')),true,'holding B ends the test without leaving the page');
 await page.getByRole('button',{name:'Test controller',exact:true}).click();await pickNone();assert.equal(await testOn(),true);
 await page.evaluate(()=>{pad.buttons[8]={pressed:true,value:1};pad.buttons[9]={pressed:true,value:1};});
 await page.waitForFunction(()=>!document.body.dataset.controllerTesting,null,{timeout:3000});
 await page.evaluate(()=>{pad.buttons[8]={pressed:false,value:0};pad.buttons[9]={pressed:false,value:0};});
 await page.screenshot({path:path.join(out,'polish-controllers.png')});
 await page.goto(base+'/settings.html#audio');await page.getByRole('slider',{name:'Game volume',exact:true}).waitFor();
 assert.equal(await page.getByRole('slider',{name:'Game volume',exact:true}).isEnabled(),true);
 await page.getByRole('slider',{name:'Game volume',exact:true}).evaluate(n=>{n.value='37';n.dispatchEvent(new Event('change',{bubbles:true}));});
 await page.waitForFunction(()=>calls.some(c=>c.cmd==='dolphin_preference_set'&&c.args.key==='volume'&&c.args.value==='37'));
 assert.equal(await page.getByRole('combobox',{name:/Audio output/}).isDisabled(),true);
 await page.evaluate(()=>sessionStorage.removeItem('live'));
 await page.goto(base+'/index.html');await page.locator('#play-display').waitFor();
 await page.locator('#play-display').click();await page.getByRole('menuitemradio',{name:'Windowed',exact:true}).click();
 await page.waitForFunction(()=>calls.some(c=>c.cmd==='dolphin_preference_set'&&c.args.key==='fullscreen'&&c.args.value==='false'));
 await page.screenshot({path:path.join(out,'polish-play.png')});
 assert.deepEqual(errors,[]);
 console.log('PASS: Stats account and sync, conditional B/X correction, four-player controller navigation during play, test escape, live audio, fullscreen checkbox.');
}finally{await browser.close();await new Promise(r=>server.close(r));}
