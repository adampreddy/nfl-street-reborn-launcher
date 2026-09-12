import {chromium} from 'playwright';import {readFileSync} from 'node:fs';import assert from 'node:assert/strict';
const source=readFileSync(new URL('../crates/reborn-ui/ui/update-notice.js',import.meta.url),'utf8');
const browser=await chromium.launch({channel:'msedge',headless:true});
async function run(options){const p=await browser.newPage();await p.route('http://update.test/**',r=>r.fulfill({contentType:'text/html',body:'<main id="main"></main>'}));await p.goto('http://update.test');await p.evaluate(options=>{
 window.calls=[];window.active=!!options.active;window.tick=null;window.setInterval=fn=>{window.tick=fn;return 1};window.clearInterval=()=>{window.tick=null};
 if(options.receipt)localStorage.setItem('updates:pending-summary',JSON.stringify(options.receipt));
 window.__TAURI__={core:{invoke:async(cmd,args)=>{calls.push(cmd);if(cmd==='onboarding_progress')return {complete:true};if(cmd==='update_check')return {current:options.current||'1',latest:'2',notes:'New features',state:options.current==='2'?'current':'available'};if(cmd==='update_policy')return {automatic:options.automatic};if(cmd==='update_download')return {path:'verified-installer.exe'};if(cmd==='session_active')return window.active;if(cmd==='update_apply')return await new Promise(resolve=>{window.installed=resolve});throw Error(cmd);}}};
 },options);await p.addScriptTag({content:source});await p.waitForTimeout(60);return p;}
let p=await run({automatic:false});assert.equal(await p.getByRole('button',{name:'View update'}).count(),1);assert.equal(await p.evaluate(()=>calls.includes('update_download')),false);await p.close();
p=await run({automatic:true,active:true});assert.equal(await p.evaluate(()=>calls.includes('update_download')),true);assert.equal(await p.evaluate(()=>calls.includes('update_apply')),false);await p.evaluate(()=>{window.active=false;window.tick()});await p.waitForFunction(()=>window.installed);await p.evaluate(()=>window.tick?.());assert.equal(await p.evaluate(()=>calls.filter(c=>c==='update_apply').length),1);await p.evaluate(()=>window.installed({}));await p.close();
p=await run({automatic:false,current:'2',receipt:{version:'2',notes:'New features'}});await p.getByText('Updated to Reborn 2. New features',{exact:true}).waitFor();await p.getByRole('button',{name:'Got it'}).click();assert.equal(await p.evaluate(()=>localStorage.getItem('updates:pending-summary')),null);await p.close();
// A launcher left open checks again on its timer: nothing new at start,
// then the timer fires and the check runs a second time and draws the
// notice once something is available.
p=await run({automatic:false,current:'2'});
assert.equal(await p.evaluate(()=>calls.filter(c=>c==='update_check').length),1,'one check at start');
assert.equal(await p.getByRole('button',{name:'View update'}).count(),0,'nothing to show while current');
await p.evaluate(()=>{window.__TAURI__.core.invoke=(orig=>async(cmd,args)=>{calls.push(cmd);if(cmd==='update_check')return {current:'2',latest:'3',notes:'',state:'available'};return orig(cmd,args);})(window.__TAURI__.core.invoke);window.tick();});
await p.getByRole('button',{name:'View update'}).waitFor();
assert.equal(await p.evaluate(()=>calls.filter(c=>c==='update_check').length),2,'the timer checked again');
await p.evaluate(()=>window.tick());await p.waitForTimeout(60);
assert.equal(await p.getByRole('button',{name:'View update'}).count(),1,'one notice, not one per tick');
await p.close();
console.log('PASS: ask-first downloads nothing; automatic updates wait for game exit and install once; installed update summary is dismissible; the timer checks again and draws one notice.');await browser.close();
