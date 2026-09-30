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
  window.pads=[];Object.defineProperty(navigator,'getGamepads',{value:()=>window.pads});document.hasFocus=()=>true;
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
          // A refused apply comes back as an answer, not a throw: phase "problem" with the core's reason.
          case "music_apply": if(window.__musicApplyRefused){status.phase='problem';status.note=window.__musicApplyRefused;return status;}status.phase='building';status.progress={done:5,total:10};return status;
          case "music_dismiss": if(status.phase==='done'||status.phase==='problem'){status.phase='idle';status.note='';}return status;
          case "music_set_enabled":status.enabled=args.on;return status;
          case "music_pack_preview":return {name:'Test mix',author:'A friend',songs:[{slot:2,title:'Track two',artist:'Artist',replaces:'Original song 2'}]};
          case "music_stage_pack": status.slots[1].pending={kind:'pack',title:'Track two',artist:'Artist'};return status;
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
 // The save bar (spec §7, §8.4): its own lane under the page, the pending count, the last change named.
 await page.locator('.save-lane.open').waitFor();
 assert.match(await page.locator('.save-bar').innerText(),/1 pending change[\s\S]*Song 3 restored[\s\S]*Discard[\s\S]*Apply changes/);
 await page.waitForTimeout(400);
 // Real geometry (P3-12): the lane sits below the page's content row, wholly inside the window, and the
 // list scrolled to its end stops above the bar (the bar never covers a song).
 const edge=await page.evaluate(()=>{const m=document.getElementById('main'),l=document.getElementById('save-lane'),b=document.querySelector('.save-bar');m.scrollTop=m.scrollHeight;const last=document.querySelector('.mz-track:last-child').getBoundingClientRect(),mr=m.getBoundingClientRect(),lr=l.getBoundingClientRect(),br=b.getBoundingClientRect();m.scrollTop=0;return {mainBottom:mr.bottom,laneTop:lr.top,laneH:lr.height,barTop:br.top,barBottom:br.bottom,lastBottom:last.bottom,h:innerHeight};});
 assert.ok(edge.laneH>40,'the lane is open and has height: '+edge.laneH);
 assert.ok(edge.laneTop>=edge.mainBottom-0.5,'the lane starts below the content row');
 assert.ok(edge.mainBottom<edge.h-40,'the content row gave the bar its room');
 assert.ok(edge.barTop>=edge.laneTop&&edge.barBottom<=edge.h,'the bar is inside its lane and the window');
 assert.ok(edge.lastBottom<=edge.barTop,'the last song scrolled into view ends above the bar');
 assert.equal(await page.getByRole('button',{name:'Add song',exact:true}).count(),0,'the duplicate Add song is gone (spec §8.4)');
 // P3-14: staged songs live in the core and survive navigation, so the bar never traps the player.
 const leaves=sel=>page.evaluate(sel=>{let seen=null;const l=e=>{seen=e.defaultPrevented;e.preventDefault();};window.addEventListener('click',l);document.querySelector(sel).click();window.removeEventListener('click',l);return seen;},sel);
 assert.equal(await page.locator('#save-lane[data-keeps]').count(),1,'the Music lane is marked keeps');
 assert.equal(await leaves('.shell-nav a[href="mods.html"]'),false,'a sidebar click with a pending change is not intercepted');
 // The header row: the switch, then Reset, Import pack, Export pack and Add songs….
 assert.equal(await page.locator('#music-enabled[role=switch]').getAttribute('aria-checked'),'true');
 assert.deepEqual(await page.locator('.music-toolbar-actions button').allInnerTexts(),['Reset','Import pack','Export pack','Add songs…']);
 assert.equal(await page.getByRole('button',{name:'Reset soundtrack',exact:true}).count(),1);
 // A pending row: "· Pending" ends its source line, and Undo is its only action; no length while pending.
 assert.match(await page.locator('[data-slot="3"] .mz-src').innerText(),/· Pending$/);
 assert.deepEqual(await page.locator('[data-slot="3"] .mz-col-actions button').allInnerTexts(),['Undo']);
 assert.equal(await page.locator('[data-slot="3"] .mz-col-time').innerText(),'');
 // A custom song: Restore original and Replace. The disc's own song: Replace.
 assert.deepEqual(await page.locator('[data-slot="21"] .mz-col-actions button').allInnerTexts(),['Restore original','Replace']);
 assert.deepEqual(await page.locator('[data-slot="1"] .mz-col-actions button').allInnerTexts(),['Replace']);
 assert.match(await page.locator('[data-slot="21"] .mz-src').innerText(),/Alicia Keys · Your song/);
 assert.match(await page.locator('[data-slot="20"] .mz-src').innerText(),/^Cole Brown · From friend\.nsrmusic$/);
 assert.match(await page.locator('[data-slot="1"] .mz-src').innerText(),/^On your disc$/);
 // The order arrows are the page's own small arrows, not 38px default buttons (P3-5).
 const arrow=await page.getByRole('button',{name:'Move song 2 up',exact:true}).boundingBox();
 assert.ok(arrow.height<=16,'an order arrow is a small arrow: '+arrow.height);
 await page.getByRole('button',{name:'Replace song 1',exact:true}).click();
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
 assert.match(await page.locator('[data-slot="2"] .mz-src').innerText(),/^Replacing Original song 2 · Pending$/);
 assert.match(await page.locator('.save-bar').innerText(),/2 pending changes[\s\S]*Song 2 replaced/);
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
 assert.match(await page.locator('[data-slot="21"] .mz-src').innerText(),/^Restoring the original · Pending$/);
 assert.match(await page.locator('.save-bar').innerText(),/Song 21 restored/);
 // A stale last change is never named (P3-12): Undo song 21, then song 3 (restored at load) is named again.
 await page.getByRole('button',{name:'Undo changes to song 21',exact:true}).click();
 assert.doesNotMatch(await page.locator('.save-bar').innerText(),/Song 21/);
 assert.match(await page.locator('.save-bar .d').innerText(),/^Song 3 restored$/);
 await page.getByRole('button',{name:'Restore original song 21',exact:true}).click();
 assert.match(await page.locator('.save-bar .d').innerText(),/^Song 21 restored$/);
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
 // The bar names what changed last, never an older change that is still pending (P3-12: stale lastChange).
 assert.equal(await page.locator('.save-bar .d').innerText(),'Song 2 replaced');
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
 await page.getByRole('switch',{name:'Custom soundtrack'}).click();
 assert.ok(await page.evaluate(()=>window.__musicCalls.some(c=>c.cmd==='music_set_enabled'&&c.args.on===false)));
 await page.waitForFunction(()=>document.getElementById('music-enabled')?.getAttribute('aria-checked')==='false');
 // Review Focus 1: a failed apply says why in the bar, keeps every change pending, and gives the buttons back.
 const discard=page.locator('.save-bar').getByRole('button',{name:'Discard',exact:true});
 const pendingBefore=await page.locator('tr.mz-track.staged').count();
 await page.evaluate(()=>window.__musicFail='music_apply');
 await apply.click();
 await page.locator('.save-bar [role=alert]').waitFor();
 assert.equal(await page.locator('.save-bar [role=alert]').innerText(),'Could not save this song. Try again.');
 assert.equal(await page.locator('.save-lane.open').count(),1,'the changes stay pending');
 assert.equal(await page.locator('tr.mz-track.staged').count(),pendingBefore,'every row is still pending');
 assert.equal(await apply.isEnabled(),true,'Apply comes back after a failure');
 assert.equal(await discard.isEnabled(),true,'Discard comes back after a failure');
 await page.evaluate(()=>window.__musicFail=null);
 // The same for a refusal the core ANSWERS with (phase "problem", its reason in note), not a throw (P3-12).
 await page.evaluate(()=>window.__musicApplyRefused='The game is running. Close it, then apply your soundtrack.');
 await apply.click();
 await page.locator('.save-bar [role=alert]',{hasText:'The game is running.'}).waitFor();
 assert.equal(await page.locator('.save-bar [role=alert]').innerText(),'The game is running. Close it, then apply your soundtrack.');
 assert.equal(await page.locator('.save-lane.open').count(),1,'a refused apply keeps the changes pending');
 assert.equal(await page.locator('tr.mz-track.staged').count(),pendingBefore);
 assert.equal(await apply.isEnabled(),true,'Apply comes back after a refusal');
 assert.equal(await page.getByText('Your soundtrack is up to date.',{exact:true}).count(),0,'nothing is shown as applied');
 await page.evaluate(()=>window.__musicApplyRefused=null);
 // A rebuild that starts and then fails (the outcome arrives by polling) says why in the bar too.
 await apply.click();
 await page.getByRole('progressbar',{name:'Applying your soundtrack'}).waitFor();
 assert.equal(await page.locator('.save-bar [role=alert]').isVisible(),false,'a new attempt clears the old reason');
 await page.evaluate(()=>{window.__musicStatus.phase='problem';window.__musicStatus.progress=null;window.__musicStatus.note='There is not enough free space to rebuild the game.';});
 await page.locator('.save-bar [role=alert]',{hasText:'not enough free space'}).waitFor({timeout:4000});
 assert.equal(await page.locator('.save-lane.open').count(),1);
 assert.equal(await apply.isEnabled(),true,'Apply comes back after a failed rebuild');
 await page.getByRole('button',{name:'Dismiss',exact:true}).click();
 await page.waitForFunction(()=>document.querySelector('.save-bar [role=alert]')?.hidden===true);
 await apply.click();
 await page.getByRole('progressbar',{name:'Applying your soundtrack'}).waitFor();
 // Review Focus 3: while the core builds, Apply AND Discard are held (a second Apply would start a second rebuild).
 assert.equal(await apply.isDisabled(),true);
 assert.equal(await discard.isDisabled(),true,'Discard waits for the build too');
 // P3-2: with both buttons held the bar itself takes focus (pointAtSaveBar / toSaveBar never focus nothing).
 assert.equal(await page.locator('.save-bar').getAttribute('tabindex'),'-1');
 await page.evaluate(()=>document.querySelector('.shell-nav a.current').focus());
 await page.evaluate(()=>window.pointAtSaveBar());
 assert.equal(await page.evaluate(()=>document.activeElement?.classList.contains('save-bar')),true,'the locked bar takes focus');
 // P3-14: a whole rebuild with both buttons held never traps the player either.
 assert.equal(await leaves('.shell-nav a[href="controls.html"]'),false,'a sidebar click during a build is not intercepted');
 assert.ok(await page.evaluate(()=>window.__musicCalls.some(c=>c.cmd==='music_apply'&&c.args.confirm===true)));
 await page.evaluate(()=>{window.__musicStatus.phase='done';window.__musicStatus.slots.forEach(s=>s.pending=null);});
 await page.getByText('Your soundtrack is up to date.',{exact:true}).waitFor({timeout:4000});
 await page.waitForFunction(()=>!document.querySelector('.save-lane.open'),null,{timeout:4000});
 for(const width of [800,1280]){
  await page.setViewportSize({width,height:650});
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
  // P3-10: the actions may wrap, never clip: every Replace / Restore original / Undo sits wholly inside its cell.
  const clipped=await page.evaluate(()=>[...document.querySelectorAll('.mz-col-actions')].flatMap(td=>{const c=td.getBoundingClientRect();return [...td.querySelectorAll('button')].filter(b=>{const r=b.getBoundingClientRect();return r.left<c.left-0.5||r.right>c.right+0.5||r.top<c.top-0.5||r.bottom>c.bottom+0.5;}).map(b=>b.getAttribute('aria-label'));}));
  assert.deepEqual(clipped,[],`action buttons outside their cell at ${width}px`);
  await page.getByRole('button',{name:'Replace song 1',exact:true}).click();
  await dialog.getByLabel('Title',{exact:true}).waitFor();
  assert.ok(await dialog.evaluate(n=>n.scrollWidth<=n.clientWidth));
  await page.screenshot({path:`${OUT}/music-dialog-${width}.png`});
  await dialog.getByRole('button',{name:'Cancel',exact:true}).click();
 }
 assert.deepEqual(errors,[]);
 // P3-14 with a pad: a change pending (the core keeps it), B from the page, then B from the sidebar leaves Music.
 await page.getByRole('button',{name:'Restore original song 21',exact:true}).click();
 await page.locator('.save-lane.open').waitFor();
 await page.evaluate(()=>{window.pads=[{id:'Test pad',index:0,connected:true,mapping:'standard',axes:[0,0,0,0],buttons:Array.from({length:18},()=>({pressed:false,value:0}))}];window.dispatchEvent(new Event('gamepadconnected'));});
 await page.waitForTimeout(150);
 await page.evaluate(()=>document.querySelector('.shell-nav a.current').focus());
 // Hold B until the page is gone (the release would land on the next page).
 await page.evaluate(()=>{pads[0].buttons[1]={pressed:true,value:1};});
 await page.waitForURL(/\/index\.html$/,{timeout:4000});
 console.log('PASS: song list, add/replace/edit/reset/undo, error recovery, import/export, apply progress, switch, responsive dialogs.');
} finally {await browser.close();}
