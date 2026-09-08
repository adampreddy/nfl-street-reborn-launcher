// Exercise the actual Tauri preference writer only against an explicitly named preview profile.
import {chromium} from 'playwright';
import {readFileSync,writeFileSync} from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
const root=process.argv[2];
if(!root||!path.isAbsolute(root))throw Error('Pass the absolute isolated preview profile root.');
const browser=await chromium.connectOverCDP('http://127.0.0.1:9339');
try{
 const page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri.localhost'));
 if(!page)throw Error('Native preview not found.');
 const invoke=(command,args)=>page.evaluate(({command,args})=>window.__TAURI__.core.invoke(command,args),{command,args});
 const before=await invoke('dolphin_preferences',{runtime:null});
 const relative=path.relative(root,before.user_dir);
 assert.ok(relative!==''&&!relative.startsWith('..')&&!path.isAbsolute(relative),'Native user directory must be inside the named isolated preview.');
 assert.equal(before.editable,true,'Close the preview Dolphin before testing.');
 const file=path.join(before.user_dir,'Config','Dolphin.ini');
 const original=before.values.volume;
 const next=original==='37'?'38':'37';
 try{
  await invoke('dolphin_preference_set',{runtime:null,key:'volume',value:next});
  assert.equal((await invoke('dolphin_preferences',{runtime:null})).values.volume,next);
  assert.match(readFileSync(file,'utf8'),new RegExp('^Volume\\s*=\\s*'+next+'\\s*$','m'));
  const contents=readFileSync(file,'utf8');
  for(const [key,value] of [['volume','101'],['InternalResolution','4']]){
   await assert.rejects(invoke('dolphin_preference_set',{runtime:null,key,value}));
   assert.equal(readFileSync(file,'utf8'),contents,'Rejected settings must not change the INI.');
  }
 }finally{
  await invoke('dolphin_preference_set',{runtime:null,key:'volume',value:original});
 }
 assert.equal((await invoke('dolphin_preferences',{runtime:null})).values.volume,original);
 const report={nativeWrite:true,iniPersisted:true,outOfRangeRejected:true,protectedKeyRejected:true,originalVolumeRestored:true};
 writeFileSync('target/browser-review/native-preferences.json',JSON.stringify(report,null,2));
 console.log(JSON.stringify(report));
}finally{await browser.close();}
