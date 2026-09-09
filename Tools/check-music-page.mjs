import assert from "node:assert/strict";
// Exercise music editing flows against a fake core in headless Edge.
import { chromium } from "playwright";
import { existsSync, mkdirSync, readFileSync } from "fs";

const UI = "crates/reborn-ui/ui";
const OUT = "target/review";
mkdirSync(OUT, { recursive: true });

function slot(n, extra = {}) {
  return { slot: n, title: "", artist: "", album: "", duration_ms: null, source: "original", source_name: null, pending: null, ...extra };
}
const slots = Array.from({ length: 21 }, (_, i) => slot(i + 1));
slots[20] = slot(21, { title: "Un-thinkable (I'm Ready)", artist: "Alicia Keys", album: "The Element of Freedom", duration_ms: 286_000, source: "your_file", source_name: "a1b2c3.mp3" });
slots[19] = slot(20, { title: "Ground Zero", artist: "Cole Brown", album: "Street's Disciple", duration_ms: 237_000, source: "pack", source_name: "friend.nsrmusic" });
slots[2] = slot(3, { pending: { kind: "original", title: "", artist: "", album: "" } });
const status = {
  phase: process.env.PHASE || "idle", progress: null, enabled: true, base: { kind: "original", name: null }, slots,
  pending_count: 1, note: "", image_ok: true, image_note: "", limit_seconds: 380, formats: ["mp3", "m4a", "flac", "ogg", "wav", "aiff"],
};
const inspect = {
  path: "C:\\Users\\adam\\Music\\03. Alicia Keys - Unthinkable (Official Video).mp3",
  title: "Un-thinkable (I'm Ready)", artist: "Alicia Keys", album: "The Element of Freedom",
  details_from: "lookup", duration_ms: 286_000, too_long: false, query: "Alicia Keys Unthinkable",
  candidates: [
    { title: "Un-thinkable (I'm Ready)", artist: "Alicia Keys", album: "The Element of Freedom", duration_ms: 286_000, source: "iTunes" },
    { title: "Un-thinkable (I'm Ready) [Remix]", artist: "Alicia Keys feat. Drake", album: "Single", duration_ms: 271_000, source: "iTunes" },
  ],
  note: "Details found online from the file name. Check them.",
};

const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: Number(process.env.W || 1280), height: Number(process.env.H || 900) } });
await page.route("http://music.test/**", (route) => {
  const url = new URL(route.request().url());
  const rel = decodeURIComponent(url.pathname === "/" ? "/music.html" : url.pathname);
  const file = UI + rel;
  if (!existsSync(file)) return route.fulfill({ status: 404, body: "" });
  const ct = rel.endsWith(".js") ? "text/javascript" : rel.endsWith(".css") ? "text/css" : rel.endsWith(".html") ? "text/html" : rel.endsWith(".png") ? "image/png" : rel.endsWith(".woff2") ? "font/woff2" : rel.endsWith(".svg") ? "image/svg+xml" : "application/octet-stream";
  return route.fulfill({ contentType: ct, body: readFileSync(file) });
});
await page.addInitScript(({ status, inspect }) => {
  window.__musicStatus=status;window.__musicCalls=[];window.__musicInspect=inspect;
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        window.__musicCalls.push({cmd,args});
        if(window.__musicFail===cmd)throw new Error('Could not save this song. Try again.');
        switch (cmd) {
          case "music_stage_file":
          case "music_stage_details": status.slots[args.slot-1].pending={kind:'custom',title:args.title,artist:args.artist,album:args.album};status.pending_count=status.slots.filter(s=>s.pending).length;return status;
          case "music_unstage": for(const s of status.slots)if(args.slot===null||s.slot===args.slot)s.pending=null;return status;
          case "music_stage_stock": status.slots[args.slot-1].pending={kind:'original'};return status;
          case "music_apply": status.phase='building';status.progress={done:5,total:10};return status;
          case "music_set_enabled":status.enabled=args.on;return status;
          case "music_pack_preview":return {name:'Test mix',author:'A friend',songs:[{slot:2,title:'Track two',artist:'Artist',replaces:'Original song 2'}]};
          case "music_stage_pack": status.slots[1].pending={kind:'custom',title:'Track two',artist:'Artist'};return status;
          case "music_export":return 'C:/Music/My Mix.nsrmusic';
          case "music_status": return status;
          case "pick_path": return args?.kind === "audio" ? inspect.path : args?.kind === "pack" ? "C:/Music/Mix.nsrmusic" : "C:/Music/My Mix.nsrmusic";
          case "music_inspect": return window.__musicInspect;
          case "music_lookup": return inspect.candidates;
          default: return status;
        }
      },
    },
  };
}, { status, inspect });

const errors=[];page.on('pageerror',e=>errors.push(e.message));
try {
 await page.goto('http://music.test/music.html');
 await page.locator('.mz-track').first().waitFor();
 assert.equal(await page.locator('.mz-track').count(),21);
 assert.equal(await page.locator('.mz-step').count(),0);
 const apply=page.getByRole('button',{name:'Apply changes',exact:true});
 assert.equal(await apply.count(),1);
 assert.ok((await apply.boundingBox()).y<900,'Apply is visible without scrolling through the list');
 await page.getByRole('button',{name:'Add song',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'Song details',exact:true});
 await dialog.getByLabel('Title',{exact:true}).waitFor();
 assert.equal(await dialog.getByLabel('Title',{exact:true}).inputValue(),inspect.title);
 assert.equal(await dialog.getByLabel('Replace song',{exact:true}).inputValue(),'1');
 assert.equal(await dialog.locator('.music-extra').getAttribute('open'),null);
 await dialog.getByLabel('Title',{exact:true}).fill('My edited song');
 await dialog.getByLabel('Replace song',{exact:true}).selectOption('2');
 await page.evaluate(()=>window.__musicFail='music_stage_file');
 await dialog.getByRole('button',{name:'Add song',exact:true}).click();
 await dialog.getByRole('alert').waitFor();
 assert.equal(await dialog.getByLabel('Title',{exact:true}).inputValue(),'My edited song','Keep edits after a failed save');
 await page.evaluate(()=>window.__musicFail=null);
 await dialog.getByRole('button',{name:'Add song',exact:true}).click();
 await dialog.waitFor({state:'detached'});
 assert.match(await page.locator('[data-slot="2"]').innerText(),/My edited song/);
 let request=await page.evaluate(()=>window.__musicCalls.find(c=>c.cmd==='music_stage_file').args);
 assert.equal(request.slot,2);assert.equal(request.path,inspect.path);
 await page.getByRole('button',{name:'Undo changes to song 2',exact:true}).click();
 await page.getByRole('button',{name:'Replace song 21',exact:true}).click();
 await dialog.getByLabel('Replace song',{exact:true}).waitFor();
 assert.equal(await dialog.getByLabel('Replace song',{exact:true}).inputValue(),'21');
 await dialog.getByRole('button',{name:'Cancel',exact:true}).click();
 await page.getByRole('button',{name:/Edit details for song 21:/}).click();
 await dialog.getByLabel('Title',{exact:true}).fill('Updated title');
 await dialog.getByRole('button',{name:'Save details',exact:true}).click();
 await dialog.waitFor({state:'detached'});
 assert.ok(await page.evaluate(()=>window.__musicCalls.some(c=>c.cmd==='music_stage_details'&&c.args.slot===21&&c.args.title==='Updated title')));
 await page.getByRole('button',{name:'Undo changes to song 21',exact:true}).click();
 await page.getByRole('button',{name:'Restore original song 21',exact:true}).click();
 assert.match(await page.locator('[data-slot="21"]').innerText(),/Restore original/);
 await page.getByRole('button',{name:'Import pack',exact:true}).click();
 await page.getByRole('dialog',{name:'Import a pack'}).getByRole('button',{name:'Import songs'}).click();
 assert.match(await page.locator('[data-slot="2"]').innerText(),/Track two/);
 await page.getByRole('button',{name:'Export pack',exact:true}).click();
 const pack=page.getByRole('dialog',{name:'Share a pack'});
 for(const checkbox of await pack.getByRole('checkbox').all())await checkbox.uncheck();
 await pack.getByRole('button',{name:'Save pack'}).click();
 await pack.getByRole('alert').waitFor();
 await pack.getByRole('checkbox').first().check();
 await pack.getByLabel('Name',{exact:true}).fill('My mix');
 await pack.getByRole('button',{name:'Save pack'}).click();
 await pack.waitFor({state:'detached'});
 assert.ok(await page.evaluate(()=>window.__musicCalls.some(c=>c.cmd==='music_export'&&c.args.name==='My mix')));
 await page.getByRole('switch',{name:'Custom soundtrack'}).uncheck();
 assert.ok(await page.evaluate(()=>window.__musicCalls.some(c=>c.cmd==='music_set_enabled'&&c.args.on===false)));
 await apply.click();
 await page.getByRole('progressbar',{name:'Applying your soundtrack'}).waitFor();
 assert.equal(await apply.isDisabled(),true);
 assert.ok(await page.evaluate(()=>window.__musicCalls.some(c=>c.cmd==='music_apply'&&c.args.confirm===true)));
 await page.evaluate(()=>{window.__musicStatus.phase='done';window.__musicStatus.slots.forEach(s=>s.pending=null);});
 await page.getByText('Your soundtrack is up to date.',{exact:true}).waitFor({timeout:4000});
 for(const width of [800,1280]){
  await page.setViewportSize({width,height:650});
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  await page.getByRole('button',{name:'Add song',exact:true}).click();
  await dialog.getByLabel('Title',{exact:true}).waitFor();
  assert.ok(await dialog.evaluate(n=>n.scrollWidth<=n.clientWidth));
  await page.screenshot({path:`${OUT}/music-dialog-${width}.png`});
  await dialog.getByRole('button',{name:'Cancel',exact:true}).click();
 }
 assert.deepEqual(errors,[]);
 console.log('PASS: song list, add/replace/edit/reset/undo, error recovery, import/export, apply progress, switch, responsive dialogs.');
} finally {await browser.close();}
