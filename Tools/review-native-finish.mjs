import {chromium} from 'playwright';
import {writeFileSync} from 'node:fs';
import assert from 'node:assert/strict';
const browser=await chromium.connectOverCDP('http://127.0.0.1:9339');
try{
 const pages=browser.contexts().flatMap(c=>c.pages());const page=pages.find(p=>p.url().includes('tauri.localhost'))||pages[0];if(!page)throw Error('Launcher WebView not found');
 const base=page.url();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const report={pages:[],preferences:null};
 for(const file of ['settings.html','mods.html','controls.html','online.html']){
  await page.goto(new URL(file,base).href);await page.waitForTimeout(1200);
  if(file==='settings.html'){
   await page.getByRole('button',{name:'Audio',exact:true}).click();
   report.preferences=await page.evaluate(async()=>{const r=await window.__TAURI__.core.invoke('dolphin_preferences',{runtime:null});return {editable:r.editable,readonly_reason:r.readonly_reason,keys:Object.keys(r.values),volume:r.values.volume,backend:r.values.audio_backend};});
   assert.ok(report.preferences.keys.includes('slot_a')&&report.preferences.keys.includes('volume'));
  }
  report.pages.push({file,overflow:await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),heading:await page.locator('h1').innerText()});
  await page.screenshot({path:'target/browser-review/native-'+file.replace('.html','')+'.png'});
 }
 assert.deepEqual(errors,[]);assert.ok(report.pages.every(p=>!p.overflow));writeFileSync('target/browser-review/native-finish.json',JSON.stringify(report,null,2));console.log(JSON.stringify(report));
}finally{await browser.close();}
