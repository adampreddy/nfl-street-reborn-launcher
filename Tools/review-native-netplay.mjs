// Prepare a real manual Netplay host through the launcher, inside an explicit preview profile.
import {chromium} from 'playwright';
import path from 'node:path';
import {writeFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const root=process.argv[2];
if(!root||!path.isAbsolute(root))throw Error('Pass the absolute isolated preview root.');
const browser=await chromium.connectOverCDP('http://127.0.0.1:9339');
try{
 const page=browser.contexts().flatMap(c=>c.pages()).find(p=>p.url().includes('tauri.localhost'));
 if(!page)throw Error('Native preview not found.');
 const prefs=await page.evaluate(async()=>{const paths=await window.__TAURI__.core.invoke('settings_paths');return window.__TAURI__.core.invoke('dolphin_preferences',{runtime:paths.runtime});});
 const relative=path.relative(root,prefs.user_dir);
 assert.ok(relative!==''&&!relative.startsWith('..')&&!path.isAbsolute(relative),'Use only the named preview profile.');
 assert.equal(prefs.editable,true,'Close Dolphin before starting this review.');
 if(process.argv.includes('--finish')){
  assert.equal(await page.evaluate(()=>window.__TAURI__.core.invoke('session_active')),false);
  assert.ok(page.url().includes('online.html'),'Keep the original handoff page open to verify its recovery.');
  await page.getByRole('button',{name:'Continue in Dolphin',exact:true}).waitFor();
  assert.equal(await page.getByRole('button',{name:'Continue in Dolphin',exact:true}).isEnabled(),true);
  assert.match(await page.locator('.settings-status').innerText(),/Dolphin is closed/);
  const report={sessionGuardReleased:true,formEnabled:true,status:await page.locator('.settings-status').innerText()};
  writeFileSync('target/browser-review/native-netplay-recovery.json',JSON.stringify(report,null,2));
  console.log(JSON.stringify(report));
 }else{
 await page.goto(new URL('online.html',page.url()).href);
 await page.getByRole('button',{name:'Host a game',exact:true}).click();
 await page.getByLabel('Player name',{exact:true}).fill('Reborn Host Test');
 await page.getByLabel('Connection',{exact:true}).selectOption('direct');
 await page.getByLabel('Port',{exact:true}).fill('26261');
 await page.getByRole('button',{name:'Continue in Dolphin',exact:true}).click();
 await page.getByRole('heading',{name:'Continue in Dolphin',exact:true}).waitFor({timeout:120000});
 const active=await page.evaluate(()=>window.__TAURI__.core.invoke('session_active'));
 assert.equal(active,true,'The launcher must retain the active-session guard.');
 const report={hostPrepared:true,sessionGuardActive:active,instructions:await page.locator('.settings-status').innerText()};
 writeFileSync('target/browser-review/native-netplay-handoff.json',JSON.stringify(report,null,2));
 await page.screenshot({path:'target/browser-review/native-netplay-handoff.png'});
 console.log(JSON.stringify(report));
 }
}finally{await browser.close();}
