// The content update notice on the Play page, driven through a fake Tauri:
// ask-first never fetches; automatic downloads, waits for the game to close,
// installs exactly once, and leaves a dismissible summary; up to date draws
// nothing; a summary left by an earlier page is shown and dismissible.
//
//     node Tools/check-content-notice.mjs
import {chromium} from 'playwright';import {readFileSync} from 'node:fs';import assert from 'node:assert/strict';
const source=readFileSync(new URL('../crates/reborn-ui/ui/content-notice.js',import.meta.url),'utf8');
const browser=await chromium.launch({channel:'msedge',headless:true});
async function run(options){const p=await browser.newPage();await p.route('http://content.test/**',r=>r.fulfill({contentType:'text/html',body:'<main id="main"></main>'}));await p.goto('http://content.test');await p.evaluate(options=>{
 window.calls=[];window.active=!!options.active;window.tick=null;window.setInterval=fn=>{window.tick=fn;return 1};window.clearInterval=()=>{window.tick=null};
 let phase=options.phase||'idle';
 const notes={idle:'Not checked yet.',up_to_date:'Everything is current.',available:'Update available: 210 MB across 12 file(s).',staged:'Update ready.',installed:'Updated: textures 1.0.1. Roster applied: 569 players, 51 teams.'};
 const view=()=>({phase,origin:'o',release:'3.0.0 sequence 3',components:[],download_bytes:210*1048576,download_files:12,repair_only:false,note:notes[phase]||phase,installed:phase==='installed'?{components:['textures 1.0.1'],game_patch:null,roster:'Roster applied: 569 players, 51 teams.',problems:[],note:notes.installed}:null});
 window.__TAURI__={core:{invoke:async(cmd,args)=>{calls.push(cmd);
  if(cmd==='onboarding_progress')return {complete:true};
  if(cmd==='content_status')return view();
  if(cmd==='content_check'){phase=options.offered===false?'up_to_date':'available';return view();}
  if(cmd==='update_policy')return {automatic:options.automatic};
  if(cmd==='content_fetch'){phase='staged';return view();}
  if(cmd==='session_active')return window.active;
  if(cmd==='content_promote'){if(!(args&&args.confirm===true))throw Error('content_promote without confirm');return await new Promise(resolve=>{window.installed=()=>{phase='installed';resolve(view());};});}
  if(cmd==='content_dismiss'){phase='idle';return view();}
  throw Error(cmd);}}};
 },options);await p.addScriptTag({content:source});await p.waitForTimeout(60);return p;}

// Ask first: the notice offers a look, and nothing is downloaded.
let p=await run({automatic:false});
assert.equal(await p.getByRole('button',{name:'View update'}).count(),1);
assert.equal(await p.evaluate(()=>calls.includes('content_fetch')),false);
await p.close();

// Automatic, game running: downloads now, waits for the game to close, installs once, summary dismissible.
p=await run({automatic:true,active:true});
await p.waitForFunction(()=>calls.includes('content_fetch'));
assert.equal(await p.evaluate(()=>calls.includes('content_promote')),false);
await p.getByText('Installs the next time you close the game',{exact:false}).waitFor();
await p.evaluate(()=>{window.active=false;window.tick()});
await p.waitForFunction(()=>window.installed);
await p.evaluate(()=>window.tick?.());
assert.equal(await p.evaluate(()=>calls.filter(c=>c==='content_promote').length),1);
await p.evaluate(()=>window.installed());
await p.getByText('Updated: textures 1.0.1. Roster applied: 569 players, 51 teams.',{exact:true}).waitFor();
await p.getByRole('button',{name:'Got it'}).click();
await p.waitForFunction(()=>calls.includes('content_dismiss'));
assert.equal(await p.locator('.update-notice').count(),0);
await p.close();

// Up to date: nothing is drawn, and the check ran once.
p=await run({automatic:true,offered:false});
assert.equal(await p.locator('.update-notice').count(),0);
assert.equal(await p.evaluate(()=>calls.filter(c=>c==='content_check').length),1);
await p.close();

// A summary left by an earlier page is shown and dismissible, without a new check.
p=await run({automatic:false,phase:'installed'});
await p.getByText('Updated: textures 1.0.1. Roster applied: 569 players, 51 teams.',{exact:true}).waitFor();
assert.equal(await p.evaluate(()=>calls.includes('content_check')),false);
await p.getByRole('button',{name:'Got it'}).click();
await p.waitForFunction(()=>calls.includes('content_dismiss'));
await p.close();

console.log('PASS: ask-first fetches nothing; automatic content updates download, wait for game exit and install once; the installed summary is shown and dismissible.');
await browser.close();
