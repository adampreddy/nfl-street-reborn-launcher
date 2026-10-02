import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';

const root=path.resolve(import.meta.dirname,'..'),ui=path.join(root,'crates/reborn-ui/ui');
const source=readFileSync(path.join(root,'Tools/check-shell-pages.mjs'),'utf8');
const fixture=vm.runInNewContext(source.slice(source.indexOf('const FULL ='),source.indexOf('// Every field null'))+'\nFULL');
Object.assign(fixture,{
  session_active:false,settings_paths:{runtime:null,game:null,userDir:null,legacyRoot:null},
  onboarding_progress:{complete:true,next:{kind:'already_complete'},summary:{}},
  update_check:{state:'current',current:'3.0.1'},update_activity:{downloading:false,ready:null},update_policy:{automatic:false},
  content_status:{phase:'up_to_date'},content_check:{phase:'up_to_date'},netplay_status:{active:false,phase:'idle'},
});
const before=process.argv.includes('--before');
const originals=new Map(before?['home.js','app-shell.js'].map(name=>[name,execFileSync('git',['show',`0cda5c3:crates/reborn-ui/ui/${name}`],{cwd:root})]):[]);
const mime={'.html':'text/html','.js':'text/javascript','.css':'text/css','.png':'image/png','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server=createServer((req,res)=>{try{
  const pathname=decodeURIComponent(new URL(req.url,'http://test').pathname),file=path.resolve(ui,'.'+pathname);
  if(!file.startsWith(ui+path.sep))throw Error('outside');
  res.writeHead(200,{'Content-Type':mime[path.extname(file)]||'application/octet-stream'});
  res.end(originals.get(pathname.slice(1))||readFileSync(file));
}catch{res.writeHead(404);res.end();}});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
const base=`http://127.0.0.1:${server.address().port}`;
const browser=await chromium.launch({channel:'msedge',headless:true});
const context=await browser.newContext({viewport:{width:1280,height:800}});
await context.addInitScript(data=>{
  window.live=false;window.reads=0;window.calls=[];window.externalDolphin=false;
  window.__TAURI__={core:{invoke:async(cmd,args={})=>{
    calls.push(cmd);
    if(cmd==='session_active'){
      reads++;
      // A transient writer was sampled by the old home loader, while the
      // shared poller only saw false. The loader then kept true forever.
      if(location.search.includes('late'))return reads===2;
      return live;
    }
    if(cmd==='home_state'&&location.search.includes('slow'))await new Promise(resolve=>{window.releaseHome=resolve;});
    if(cmd==='dolphin_preferences')return {editable:!externalDolphin,live_editable:[],readonly_reason:externalDolphin?'Close Dolphin before changing its settings.':null,values:{volume:'40',muted:'False',fullscreen:'True',audio_backend:'Cubeb',audio_latency:'20',audio_stretch:'False'}};
    if(cmd==='netplay_pending_join')return null;
    if(cmd==='ui_notices')return [];
    return data[cmd]??null;
  }}};
},fixture);
const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(e.message));page.setDefaultTimeout(8000);
try{
  await page.goto(base+'/index.html?late');
  await page.waitForFunction(()=>reads>=3);
  await page.getByRole('button',{name:'Play now',exact:true}).waitFor({timeout:3500});
  assert.ok(await page.getByRole('button',{name:'Play now',exact:true}).isEnabled(),'An idle snapshot clears a transient active read');

  await page.goto(base+'/index.html');await page.getByRole('button',{name:'Play now',exact:true}).waitFor();
  await page.evaluate(()=>live=true);await page.getByRole('button',{name:'Playing',exact:true}).waitFor();
  assert.ok(await page.getByRole('button',{name:'Playing',exact:true}).isDisabled(),'A real session still prevents duplicate launches');
  await page.evaluate(()=>live=false);await page.getByRole('button',{name:'Play now',exact:true}).waitFor();
  const loads=await page.evaluate(()=>calls.filter(cmd=>cmd==='home_state').length);
  await page.waitForTimeout(3200);
  assert.equal(await page.evaluate(()=>calls.filter(cmd=>cmd==='home_state').length),loads,'Repeated idle snapshots do not rescan the entire Play page');

  await page.goto(base+'/index.html?slow');await page.waitForFunction(()=>!!window.releaseHome);
  await page.evaluate(()=>live=true);await page.waitForTimeout(1700);
  await page.evaluate(()=>live=false);await page.waitForTimeout(1700);
  await page.evaluate(()=>releaseHome());
  await page.getByRole('button',{name:'Play now',exact:true}).waitFor();
  assert.ok(await page.getByRole('button',{name:'Play now',exact:true}).isEnabled(),'A slow initial page load cannot restore stale active state');

  await page.goto(base+'/online.html');await page.getByRole('button',{name:/^Direct connection/}).click();await page.getByRole('button',{name:'Join game',exact:true}).waitFor();
  await page.evaluate(()=>live=true);await page.getByRole('button',{name:'Game session in progress',exact:true}).waitFor();
  await page.evaluate(()=>live=false);await page.getByRole('button',{name:'Join game',exact:true}).waitFor();
  assert.ok(await page.getByRole('button',{name:'Join game',exact:true}).isEnabled(),'Online unlocks when the session ends');

  await page.goto(base+'/settings.html#audio');await page.getByRole('slider',{name:'Game volume',exact:true}).waitFor();
  await page.evaluate(()=>externalDolphin=true);
  await page.waitForFunction(()=>document.querySelector('input[type=range]')?.disabled);
  await page.evaluate(()=>externalDolphin=false);
  await page.waitForFunction(()=>document.querySelector('input[type=range]')?.disabled===false);
  assert.ok(await page.getByRole('slider',{name:'Game volume',exact:true}).isEnabled(),'Settings unlock without a manual refresh after external Dolphin closes');
  await page.getByRole('slider',{name:'Game volume',exact:true}).focus();
  await page.evaluate(()=>window.originalVolume=document.activeElement);await page.waitForTimeout(1700);
  assert.ok(await page.evaluate(()=>document.activeElement===originalVolume&&originalVolume.isConnected),'Idle settings polling preserves the mounted control and focus');
  assert.deepEqual(errors,[]);
  console.log('PASS: stale active snapshots recover, real sessions stay protected, slow page loads cannot restore stale state, idle polling is cheap, Online and Settings unlock after exit.');
}finally{await browser.close();await new Promise(resolve=>server.close(resolve));}
