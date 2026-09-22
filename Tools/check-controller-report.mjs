import {chromium} from 'playwright';
import {createServer} from 'node:http';
import {readFileSync,mkdirSync,writeFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {replayReport} from './replay-controller-report.mjs';
const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const types={'.html':'text/html','.js':'text/javascript','.css':'text/css','.svg':'image/svg+xml','.png':'image/png','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{const file=path.resolve(ui,'.'+new URL(req.url,'http://test').pathname);if(!file.startsWith(ui+path.sep))throw Error();res.setHeader('Content-Type',types[path.extname(file)]||'application/octet-stream');res.end(readFileSync(file));}catch{res.writeHead(404).end();}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const browser=await chromium.launch({channel:'msedge',headless:true});
try{
 const page=await browser.newPage({viewport:{width:1280,height:800}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.clock.install();
 await page.addInitScript(()=>{
  window.calls=[];window.saveOutcome=null;window.reportStatus='ready';window.holdStart=false;
  window.pads=[{id:'USB Pad',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:16},()=>({value:0,pressed:false}))}];
  Object.defineProperty(navigator,'getGamepads',{value:()=>pads});document.hasFocus=()=>true;
  window.__TAURI__={core:{invoke:async(cmd,args={})=>{
   calls.push({cmd,args:structuredClone(args)});
   if(cmd==='launch_check')return {runtime:'runtime.exe'};
   if(cmd==='devices')return {pads:[],complete:true};
   if(cmd==='controller_slots')return {slots:[null,null,null,null]};
   if(cmd==='controls_options'||cmd==='ui_notices'||cmd==='notices_pending')return [];
   if(cmd==='session_active')return false;
   if(cmd==='controller_report_start'){
    if(holdStart)await new Promise(r=>window.releaseStart=r);
    return {status:reportStatus,devices:reportStatus==='ready'?[{device:'SDL/0/USB Pad'}]:[],context:{launcher_version:'test'}};
   }
   if(cmd==='controller_report_detect')return {status:'detected',binding:'`Hat 0 N`'};
   if(cmd==='controller_report_save'){window.saved=structuredClone(args.recording);if(window.saveError)throw Error('disk unavailable');return saveOutcome;}
   return {};
  }}};
 });
 await page.goto(`http://127.0.0.1:${server.address().port}/controls.html`);
 await page.getByRole('button',{name:'Record controller problem',exact:true}).click();
 await page.getByRole('button',{name:'Begin recording',exact:true}).waitFor();
 await page.getByLabel('Describe the problem',{exact:true}).fill('Down moves up in Game Modes.');
 mkdirSync(path.join(root,'target/browser-review'),{recursive:true});
 for(const width of [1280,800]){await page.setViewportSize({width,height:800});await page.evaluate(()=>document.fonts.ready);
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),true);
  await page.screenshot({path:path.join(root,`target/browser-review/controller-report-${width}.png`)});}
 await page.getByRole('button',{name:'Begin recording',exact:true}).click();
 await page.getByRole('button',{name:'Skip this control',exact:true}).click(); // rest
 await page.getByRole('button',{name:'Skip this control',exact:true}).click(); // up
 await page.getByRole('button',{name:'Record input',exact:true}).click();await page.clock.runFor(1501);
 await page.evaluate(()=>pads[0].buttons[12]={value:1,pressed:true});await page.clock.runFor(200);
 await page.evaluate(()=>pads=[]);await page.clock.runFor(4900);
 await page.getByRole('button',{name:'Review report',exact:true}).click();
 await page.getByRole('button',{name:'Save report',exact:true}).click();
 await page.getByText('Save canceled.',{exact:false}).waitFor();
 await page.evaluate(()=>saveError=true);await page.getByRole('button',{name:'Save report',exact:true}).click();
 await page.getByText('The report could not be saved.',{exact:false}).waitFor();
 await page.evaluate(()=>{saveError=false;saveOutcome='reborn-controller-report.json';});await page.getByRole('button',{name:'Save report',exact:true}).click();
 await page.getByText('Saved reborn-controller-report.json.',{exact:false}).waitFor();
 const recording=await page.evaluate(()=>saved),report={schema:'nsr.controller-report.v1',context:{test_fixture:true},recording};
 assert.equal(recording.steps[2].binding,'`Hat 0 N`');assert.equal(recording.steps[2].samples.some(s=>s.pads.length===0),true);
 assert.equal(replayReport(report)[2].direction_mismatch,true);
 writeFileSync(path.join(root,'target/browser-review/controller-report-fixture.json'),JSON.stringify(report,null,2));
 await page.getByRole('button',{name:'Close',exact:true}).click();
 assert.equal(await page.evaluate(()=>document.body.dataset.controllerTesting),undefined);
 // Closing during runtime startup must stop a late-created capture too.
 await page.evaluate(()=>holdStart=true);await page.getByRole('button',{name:'Record controller problem',exact:true}).click();
 await page.getByRole('button',{name:'Cancel',exact:true}).click();await page.evaluate(()=>releaseStart());
 await page.clock.runFor(50);assert.equal(await page.locator('dialog[open]').count(),0);
 assert.ok(await page.evaluate(()=>calls.filter(c=>c.cmd==='controller_report_stop').length>=3));
 // A slow device scan must not steal focus while the player describes a bug.
 await page.getByRole('button',{name:'Record controller problem',exact:true}).click();
 await page.getByLabel('Describe the problem',{exact:true}).fill('Typing while the controller scan finishes');
 await page.evaluate(()=>releaseStart());await page.clock.runFor(50);
 assert.equal(await page.getByLabel('Describe the problem',{exact:true}).evaluate(n=>n===document.activeElement),true);
 await page.getByRole('button',{name:'Cancel',exact:true}).click();
 // Runtime unavailable: browser-only reports remain possible, and Escape closes.
 await page.evaluate(()=>{holdStart=false;reportStatus='unavailable';});await page.getByRole('button',{name:'Record controller problem',exact:true}).click();
 await page.getByText('The game input reader is unavailable.',{exact:false}).waitFor();await page.keyboard.press('Escape');
 assert.equal(await page.locator('dialog[open]').count(),0);
 // Cancel during the countdown/recording; a late timer must not reopen or save it.
 await page.getByRole('button',{name:'Record controller problem',exact:true}).click();
 await page.getByRole('button',{name:'Begin recording',exact:true}).click();
 await page.getByRole('button',{name:'Record input',exact:true}).click();await page.clock.runFor(1600);
 const saves=await page.evaluate(()=>calls.filter(c=>c.cmd==='controller_report_save').length);
 await page.getByRole('button',{name:'Cancel',exact:true}).click();await page.clock.runFor(6000);
 assert.equal(await page.locator('dialog[open]').count(),0);
 assert.equal(await page.evaluate(()=>document.body.dataset.controllerTesting),undefined);
 assert.equal(await page.evaluate(()=>calls.filter(c=>c.cmd==='controller_report_save').length),saves);
 assert.equal(await page.evaluate(()=>calls.some(c=>['setup_save_mapping','controller_assign_player','controls_set'].includes(c.cmd))),false,'diagnostics never write mappings');
 assert.deepEqual(errors,[]);
 console.log('PASS: controller report prompt recording, raw/runtime evidence, disconnects, replay, save cancellation/retry, late startup cancellation, unavailable runtime, responsive layout, no mapping writes');
}finally{await browser.close();server.close();}
