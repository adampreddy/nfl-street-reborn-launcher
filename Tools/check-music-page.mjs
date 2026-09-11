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
// An original song the player renamed: the disc's audio, their banner text.
slots[4] = slot(5, { title: "My name for song 5", artist: "Still the disc", album: "", duration_ms: 200_000, source: "original", original: { title: "Song 5", artist: "Artist 5", album: "", duration_ms: 200_000 } });
// The disc's own song moved here from slot 8 by a reorder.
slots[5] = slot(6, { title: "Song 8", artist: "Artist 8", album: "", duration_ms: 180_000, source: "original_moved" });
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
          case "pick_paths": return window.__musicPickMany||["C:\\Music\\02 Second.mp3","C:\\Music\\01 First.flac","C:\\Music\\10 Tenth.m4a"];
          case "music_stage_files": status.phase='encoding';status.progress={label:`Adding song 1 of ${args.paths.length}: ${args.paths[0].split(/[\\/]/).pop()}`,done:3,total:40};return status;
          case "music_move": {
            // the same list move the core does: from lands on to, the ones between shift
            const list=status.slots.map(s=>({...s}));const i=list.findIndex(s=>s.slot===args.from);const [moved]=list.splice(i,1);let j=list.findIndex(s=>s.slot===args.to);if(args.to>args.from)j+=1;list.splice(j,0,moved);
            list.forEach((s,k)=>{const n=k+1;if(s.slot!==n){status.slots[k]={...s,slot:n,pending:{kind:s.source==='original'?'original_moved':s.source==='pack'?'pack':'your_file',title:s.title||`Song ${s.slot}`,artist:s.artist,album:s.album}};}else status.slots[k]=s;});
            status.pending_count=status.slots.filter(s=>s.pending).length;return status;
          }
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
 // A renamed original shows the player's name and offers Reset; a moved
 // original shows its own name with no "original" treatment.
 assert.match(await page.locator('[data-slot="5"]').innerText(),/My name for song 5/);
 assert.equal(await page.getByRole('button',{name:'Restore original song 5',exact:true}).count(),1);
 assert.match(await page.locator('[data-slot="6"]').innerText(),/Song 8/);
 // Reorder with the arrows: song 6 moves up onto 5.
 await page.getByRole('button',{name:'Move song 6 up',exact:true}).click();
 let move=await page.evaluate(()=>window.__musicCalls.filter(c=>c.cmd==='music_move').at(-1).args);
 assert.deepEqual(move,{from:6,to:5});
 assert.match(await page.locator('[data-slot="5"]').innerText(),/Song 8/,'the moved song is drawn in its new slot');
 assert.equal(await page.getByRole('button',{name:'Move song 1 up',exact:true}).isDisabled(),true,'the first song cannot move up');
 assert.equal(await page.getByRole('button',{name:'Move song 21 down',exact:true}).isDisabled(),true,'the last song cannot move down');
 // Reorder by dragging with the mouse (pointer events, not HTML5 drag and
 // drop, which the launcher's window never delivers): row 9 lifted and let
 // go on the upper half of row 7 lands above 7, so it becomes song 7.
 const rowBox=async n=>await page.locator(`[data-slot="${n}"]`).boundingBox();
 let b9=await rowBox(9),b7=await rowBox(7);
 await page.mouse.move(b9.x+b9.width/2,b9.y+b9.height/2);await page.mouse.down();
 await page.mouse.move(b9.x+b9.width/2,b9.y+b9.height/2+12,{steps:3});
 assert.ok(await page.locator('.mz-ghost').count()===1,'a ghost row follows the pointer once lifted');
 await page.mouse.move(b7.x+b7.width/2,b7.y+4,{steps:6});
 assert.equal(await page.locator('[data-slot="7"].drop-above').count(),1,'the landing line shows above row 7');
 await page.mouse.up();
 move=await page.evaluate(()=>window.__musicCalls.filter(c=>c.cmd==='music_move').at(-1).args);
 assert.deepEqual(move,{from:9,to:7});
 assert.equal(await page.locator('.mz-ghost').count(),0,'the ghost is gone after the drop');
 // Dragging down onto the lower half of a row lands below it (rows 2 and
 // 6 are both inside the 900 px viewport; further rows need the scroll
 // the real drag does near the window's edge).
 const b2=await rowBox(2),b6=await rowBox(6);
 await page.mouse.move(b2.x+b2.width/2,b2.y+b2.height/2);await page.mouse.down();await page.mouse.move(b2.x+b2.width/2,b2.y+b2.height/2+10,{steps:3});
 await page.mouse.move(b6.x+b6.width/2,b6.y+b6.height-4,{steps:6});
 assert.equal(await page.locator('[data-slot="6"].drop-below').count(),1,'the landing line shows below row 6');
 await page.mouse.up();
 move=await page.evaluate(()=>window.__musicCalls.filter(c=>c.cmd==='music_move').at(-1).args);
 assert.deepEqual(move,{from:2,to:6});
 // A press without movement is a click, not a drag: nothing moves.
 const b4=await rowBox(4);await page.mouse.move(b4.x+b4.width/2,b4.y+b4.height/2);await page.mouse.down();await page.mouse.up();
 assert.equal(await page.evaluate(()=>window.__musicCalls.filter(c=>c.cmd==='music_move').length),3,'no move from a plain click');
 // Several songs at once: sorted by file name, from the chosen slot on.
 await page.getByRole('button',{name:'Add songs…',exact:true}).click();
 const bulk=page.getByRole('dialog',{name:'Add songs',exact:true});
 await bulk.getByRole('heading',{name:'3 songs',exact:true}).waitFor();
 assert.deepEqual(await bulk.locator('ol li').allInnerTexts(),['01 First.flac','02 Second.mp3','10 Tenth.m4a']);
 await bulk.getByLabel('Start at song',{exact:true}).selectOption('20');
 assert.match(await bulk.getByRole('status').innerText(),/2 of these fit from song 20 on/);
 await bulk.getByLabel('Start at song',{exact:true}).selectOption('4');
 assert.match(await bulk.getByRole('status').innerText(),/songs 4 to 6/);
 await bulk.getByRole('button',{name:'Add 3 songs',exact:true}).click();
 await bulk.waitFor({state:'detached'});
 const batch=await page.evaluate(()=>window.__musicCalls.find(c=>c.cmd==='music_stage_files').args);
 assert.equal(batch.startSlot,4);assert.deepEqual(batch.paths.map(p=>p.split(/[\\/]/).pop()),['01 First.flac','02 Second.mp3','10 Tenth.m4a']);
 await page.locator('.music-progress strong',{hasText:'Adding song 1 of 3: 01 First.flac'}).waitFor();
 await page.evaluate(()=>{window.__musicStatus.phase='idle';window.__musicStatus.progress=null;});
 await page.getByRole('button',{name:'Import pack',exact:true}).waitFor({timeout:4000});
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
