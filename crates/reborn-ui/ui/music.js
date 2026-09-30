import { mountSaveBar } from "./savebar.js";

// A soundtrack list with focused editing and one apply action.
// The core owns soundtrack changes; this page presents them for review.
const bridge = window.__TAURI__ ? window.__TAURI__.core : null;

let status = null; // music_status; null until read or after a failed read
let busy = false; // one action at a time
let problem = null; // a failure, in the core's words, drawn at the top
let notice = null; // a partial answer, not a failure
let pollTimer = null; // set while the core encodes or builds

// The song being added: the InspectView for the chosen file, the fields as
// typed, the lookup candidates, and the slot picked in step 3. `editing` is a
// slot whose details are being changed without a new file.
let picked = null;
let editing = null;
let fields = { title: "", artist: "", album: "" };
let candidates = [];
let lookupNote = "";
let searchText = "";
let targetSlot = null;
// The slot being dragged to a new place in the list, while a drag is on.
let dragFrom = null;

// ---------------------------------------------------------------------------
// Talking to the core
// ---------------------------------------------------------------------------

function log(what, value) {
  const pre = document.getElementById("out");
  if (!pre) return;
  const body = typeof value === "string" ? value : JSON.stringify(value, null, 2);
  pre.textContent = `${what}\n${body}\n\n${pre.textContent}`.slice(0, 20000);
}

async function call(cmd, args = {}) {
  if (!bridge) throw new Error("the launcher core is not attached");
  const answer = await bridge.invoke(cmd, args);
  log(cmd, answer === undefined ? "ok" : answer);
  return answer;
}

function reason(e) {
  if (e === null || e === undefined) return "unknown failure";
  if (typeof e === "string") return e;
  if (e.message) return e.message;
  return JSON.stringify(e);
}

async function load() {
  try {
    status = await call("music_status");
  } catch (e) {
    status = null;
    problem = `Your soundtrack could not be read: ${reason(e)}`;
  }
}

// One action, then the fresh status. From the save bar ({ fromBar: true }) a failure is thrown back to
// the bar, which says it in its failure line (spec §7); anywhere else it is drawn at the top of the page.
async function act(cmd, args, { fromBar = false } = {}) {
  if (busy) return;
  busy = true;
  problem = null;
  notice = null;
  if (cmd === "music_apply") applyWatch = false; // a new attempt: the last one's reason no longer stands
  draw();
  const before = pendingSigs();
  let failure = null;
  try {
    const r = await call(cmd, args);
    if (r && typeof r === "object" && Array.isArray(r.slots)) status = r;
    else await load();
    noteChange(cmd, args, before);
  } catch (e) {
    if (fromBar) failure = e;
    else problem = reason(e);
  }
  busy = false;
  draw();
  schedulePoll();
  if (failure) throw failure;
}

// The change the bar names last ("Song 3 replaced"). It is kept with the pending state its song had right
// after the change, and named only while that song still has exactly that change pending: an Undo, a
// Discard, or a later change to the same song (a pack, a move) never leaves an older change named.
let lastChange = null;
const pendingSig = (n) => JSON.stringify(slotAt(n)?.pending ?? null);
const pendingSigs = () => new Map(slots().map((s) => [s.slot, JSON.stringify(s.pending ?? null)]));
function noteLast(slot, label) {
  lastChange = { slot, label, sig: pendingSig(slot) };
}
function noteChange(cmd, args, before) {
  if (cmd === "music_move") noteLast(args.to, `Song ${args.from} moved to ${args.to}`);
  else if (cmd === "music_stage_stock") noteLast(args.slot, `Song ${args.slot} restored`);
  else if (cmd === "music_apply" || cmd === "music_reset_all" || (cmd === "music_unstage" && args?.slot === null)) lastChange = null;
  else {
    // Anything else that staged songs (a pack): name the last song whose pending change is new.
    const changed = slots().filter((s) => s.pending && JSON.stringify(s.pending) !== before.get(s.slot));
    const last = changed.at(-1);
    if (last) noteLast(last.slot, `Song ${last.slot} ${PENDING_WORD[last.pending.kind] || "changed"}`);
  }
  if (cmd === "music_apply") applyWatch = true;
}
const PENDING_WORD = { original: "restored", your_file: "replaced", pack: "replaced", original_moved: "moved" };
// The save bar's list (spec §7): one line per song waiting to be applied, the one changed last at the end.
function pendingChanges() {
  const list = slots().filter((s) => s.pending).map((s) => ({ slot: s.slot, label: `Song ${s.slot} ${PENDING_WORD[s.pending.kind] || "changed"}` }));
  const last = lastChange && lastChange.sig === pendingSig(lastChange.slot) ? list.find((c) => c.slot === lastChange.slot) : null;
  if (!last) return list.map((c) => c.label);
  return [...list.filter((c) => c !== last).map((c) => c.label), lastChange.label];
}
// An Apply the core accepted reports its outcome later (music_status polling). While one is out, a
// "problem" outcome is said in the bar's failure line and the changes stay pending (Review Focus 1).
let applyWatch = false;
function applyOutcome() {
  if (!applyWatch) return;
  if (status?.phase === "problem") bar.fail(status.note || "Your soundtrack could not be applied.");
  else if (!working()) {
    applyWatch = false;
    bar.fail("");
  }
}

async function refresh() {
  problem = null;
  notice = null;
  await load();
  draw();
  schedulePoll();
}

function schedulePoll() {
  const working = status && (status.phase === "encoding" || status.phase === "building");
  if (pollTimer) {
    clearTimeout(pollTimer);
    pollTimer = null;
  }
  if (!working) return;
  pollTimer = setTimeout(async () => {
    pollTimer = null;
    await load();
    draw();
    schedulePoll();
  }, 1000);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined) continue;
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else if (k === "onclick") node.addEventListener("click", v);
    else if (k === "onchange") node.addEventListener("change", v);
    else if (k === "oninput") node.addEventListener("input", v);
    else if (k === "onkeydown") node.addEventListener("keydown", v);
    else if (k === "checked" || k === "disabled" || k === "selected") node[k] = !!v;
    else if (k === "value") node.value = v;
    else node.setAttribute(k, v);
  }
  for (const c of children) {
    if (c !== null && c !== undefined) node.append(c);
  }
  return node;
}

function mmss(ms) {
  if (typeof ms !== "number") return null;
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

// Why a song will not go in, in the same words the launcher's core uses: its
// length, the length the game fits, and what to do about it. The limit is the
// game's own stream buffer, so no amount of room on the disc changes it, and
// the number comes from the core rather than being written down here twice.
function tooLongProblem(song) {
  // guard the number, not the formatted string: mmss(0) is "0:00", which is
  // truthy, and would promise a cap of no seconds at all
  const secs = status?.limit_seconds;
  const cap = typeof secs === "number" && secs > 0 ? mmss(secs * 1000) : null;
  const mine = mmss(song?.duration_ms);
  if (!cap) return "This song is longer than the game fits in a slot. Trim it in an audio editor and add it again.";
  return `${mine ? `This song is ${mine} long. ` : ""}The game fits up to ${cap} in a slot. Trim it in an audio editor and add it again.`;
}

function slots() {
  return status && Array.isArray(status.slots) ? status.slots : [];
}

function slotAt(n) {
  return slots().find((s) => s.slot === n) || null;
}

function working() {
  return !!(status && (status.phase === "encoding" || status.phase === "building"));
}

function locked() {
  return busy || working();
}

// Where a song comes from, for the line under its name (spec §8.4).
const SOURCE_WORD = {
  original: "On your disc",
  original_moved: "On your disc, moved",
  your_file: "Your song",
  soundtrack_mod: "Soundtrack mod",
};

// The line under a song: who sings it and where it comes from; or, while a change waits to be applied,
// what it does, ending in " · Pending" (spec §8.4; a chip and Pending in the actions were rejected).
function sourceLine(song, artist) {
  const p = song.pending;
  if (p) {
    const what = p.kind === "original" ? "Restoring the original" : p.kind === "original_moved" ? "Moved here" : `Replacing ${slotTitle(song)}`;
    return [what, el("span", { class: "pend", text: " · Pending" })];
  }
  const where = song.source === "pack" ? (song.source_name ? `From ${song.source_name}` : "From a pack")
    : song.source === "soundtrack_mod" && song.source_name ? song.source_name
    : SOURCE_WORD[song.source] || "";
  return [[artist, where].filter(Boolean).join(" · ")];
}

function slotTitle(s) {
  if (!s) return "";
  if (s.title) return s.title;
  if (s.original && s.original.title) return s.original.title;
  return `Original song ${s.slot}`;
}

function slotLine(s) {
  if (!s) return "";
  const o = s.original;
  if (s.source === "original") {
    if (!(o && o.title)) return `Original song ${s.slot}`;
    return [o.title, o.artist].filter(Boolean).join(" by ") + (mmss(o.duration_ms) ? ` (${mmss(o.duration_ms)})` : "");
  }
  return [s.title || "Untitled", s.artist].filter(Boolean).join(" by ") + (mmss(s.duration_ms) ? ` (${mmss(s.duration_ms)})` : "");
}

function firstFreeSlot() {
  const free = slots().find((s) => s.source === "original" && !s.pending);
  return free ? free.slot : 21;
}

function pendingWhat(p, s) {
  if (p.kind === "original") {
    const o = s && s.original;
    return o && o.title ? `the original song (${[o.title, o.artist].filter(Boolean).join(" by ")})` : "the original song";
  }
  return [p.title, p.artist].filter(Boolean).join(" by ") || "a song";
}

function clearFlow() {
  picked = null;
  editing = null;
  fields = { title: "", artist: "", album: "" };
  candidates = [];
  lookupNote = "";
  searchText = "";
  targetSlot = null;
}

async function pickPath(kind) {
  try {
    return await call("pick_path", { kind });
  } catch (e) {
    problem = reason(e);
    draw();
    return null;
  }
}

// Song editing stays in one dialog; the soundtrack list never becomes a wizard.
let editor = null;
let editorProblem = '';
let editorOpener = null;
let extraOpen = false;
let searchBusy = false;

async function chooseFile(n) {
  if (locked()) return;
  busy = true;
  const path = await pickPath('audio');
  busy = false;
  if (!path) return;
  editorOpener = document.activeElement;
  targetSlot = typeof n === 'number' ? n : firstFreeSlot();
  editing = null; picked = {path, reading:true}; fields = {title:'',artist:'',album:''};
  editorProblem = ''; candidates = []; lookupNote = ''; extraOpen = false;
  openEditor(); renderEditor();
  try {
    picked = await call('music_inspect', {path});
    fields = {title:picked.title||'',artist:picked.artist||'',album:picked.album||''};
    candidates = picked.candidates || []; searchText = picked.query || '';
    lookupNote = picked.details_from === 'lookup' ? 'Details found online. You can edit them below.' : picked.details_from === 'tags' ? 'Details read from your file. You can edit them below.' : 'Check the song details before adding it.';
    if (picked.too_long) editorProblem = tooLongProblem(picked);
  } catch(e) { picked = {path, failed:true}; editorProblem = reason(e); }
  if (editor?.open) renderEditor();
}

function editDetails(n) {
  if (locked()) return;
  editorOpener = document.activeElement;
  const song = slotAt(n), src = song?.pending || song;
  clearFlow(); editing = n; targetSlot = n; editorProblem = ''; extraOpen = false;
  fields = {title:src?.title||'',artist:src?.artist||'',album:src?.album||''};
  openEditor(); renderEditor();
}
function openEditor() {
  if(editor?.open) return;
  editor = openDialog('Song details'); editor.classList.add('music-editor');
  editor.addEventListener('cancel', ev=>{if(busy||picked?.reading||searchBusy)ev.preventDefault();});
  editor.addEventListener('close',()=>{editor=null;clearFlow();editorOpener?.focus?.({preventScroll:true});});
  showDialog(editor);
}
function renderEditor() {
  if(!editor) return;
  const reading=!!picked?.reading, saving=busy, disabled=reading||saving||searchBusy;
  const heading=editing!==null?'Edit song details':'Add a song';
  const form=el('form',{class:'music-editor-form'});
  form.append(el('div',{class:'music-dialog-heading'},el('h2',{text:heading}),el('button',{type:'button',text:'Cancel',disabled,onclick:()=>editor.close()})));
  if(picked) form.append(el('div',{class:'music-file'},el('div',{},el('strong',{text:picked.path.split(/[\\/]/).pop()}),el('small',{text:reading?'Reading song details…':mmss(picked.duration_ms)||'Audio file'})),el('button',{type:'button',text:'Change file',disabled,onclick:()=>chooseFile(targetSlot)})));
  if(editorProblem)form.append(el('p',{class:'music-error',role:'alert',text:editorProblem}));
  if(reading){form.append(el('p',{class:'note',role:'status',text:'Getting the title, artist, and album…'}));editor.replaceChildren(form);return;}
  if(lookupNote)form.append(el('p',{class:'note',role:'status',text:lookupNote}));
  const field=(key,label)=>el('label',{class:'mz-field'},el('span',{text:label}),el('input',{id:'music-'+key,type:'text',maxlength:'49',required:key==='title'?'':null,value:fields[key],disabled,oninput:ev=>{fields[key]=ev.target.value;}}));
  form.append(el('div',{class:'mz-fields'},field('title','Title'),field('artist','Artist'),field('album','Album')));
  if(editing===null){
    const select=el('select',{id:'music-slot','aria-label':'Replace song',disabled,onchange:ev=>{targetSlot=Number(ev.target.value);}});
    for(const song of slots())select.append(el('option',{value:String(song.slot),text:`${String(song.slot).padStart(2,'0')} · ${song.pending ? pendingWhat(song.pending, song) : slotTitle(song)}`,selected:song.slot===targetSlot}));
    form.append(el('label',{class:'mz-field'},el('span',{text:'Replace song'}),select));
  }
  const extra=el('details',{class:'music-extra'});extra.open=extraOpen;
  extra.addEventListener('toggle',()=>{extraOpen=extra.open;});
  extra.append(el('summary',{text:'Find these details online'}));
  const search=el('input',{type:'search','aria-label':'Find song details',placeholder:'Song or artist',value:searchText,disabled,oninput:ev=>{searchText=ev.target.value;},onkeydown:ev=>{if(ev.key==='Enter'){ev.preventDefault();runSearch();}}});
  extra.append(el('div',{class:'mz-row'},search,el('button',{type:'button',text:searchBusy?'Searching…':'Search',disabled,onclick:runSearch})));
  if(candidates.length){const list=el('ul',{class:'mz-candidates'});for(const c of candidates)list.append(el('li',{},el('button',{type:'button',class:'mz-candidate',disabled,text:[c.title,c.artist,c.album].filter(Boolean).join(' · '),onclick:()=>{fields={title:c.title||'',artist:c.artist||'',album:c.album||''};lookupNote='Details updated.';renderEditor();}})));extra.append(list);}
  extra.append(el('p',{class:'note mz-privacy',text:'Online lookup sends the song name to iTunes or MusicBrainz. Your audio stays on this PC.'}));
  form.append(extra,el('div',{class:'music-dialog-footer'},el('p',{class:'note',text:'Save here, then apply your changes to the game.'}),el('button',{type:'submit',class:'primary',text:saving?'Saving…':editing!==null?'Save details':'Add song',disabled:disabled||!!picked?.too_long||!!picked?.failed})));
  form.addEventListener('submit',ev=>{ev.preventDefault();if(form.reportValidity())addToQueue();});
  const focus=document.activeElement?.id;
  editor.replaceChildren(form);
  if(focus)document.getElementById(focus)?.focus({preventScroll:true});
  else if(!disabled)form.querySelector('input')?.focus({preventScroll:true});
}
async function runSearch(){
  if(searchBusy||busy||!searchText.trim())return;
  searchBusy=true;lookupNote='Searching…';renderEditor();
  try{candidates=await call('music_lookup',{query:searchText.trim(),durationMs:picked&&!picked.failed?picked.duration_ms??null:slotAt(targetSlot)?.duration_ms??null});lookupNote=candidates.length?'Choose a result or edit the details yourself.':'No matches found. You can enter the details yourself.';}
  catch(e){lookupNote=reason(e);}
  finally{searchBusy=false;if(editor?.open)renderEditor();}
}
async function addToQueue(){
  if(locked()||picked?.reading||picked?.too_long||picked?.failed)return;
  busy=true;editorProblem='';renderEditor();
  const args={slot:targetSlot,title:fields.title.trim(),artist:fields.artist.trim(),album:fields.album.trim()};
  try{
    const result=await call(editing!==null?'music_stage_details':'music_stage_file',editing!==null?args:{...args,path:picked.path});
    if(result?.slots)status=result;else await load();
    lastChange={slot:args.slot,label:editing!==null?`Song ${args.slot} details changed`:`Song ${args.slot} replaced`,sig:pendingSig(args.slot)};
    editor.close();notice=null;problem=null;
  }catch(e){editorProblem=reason(e);}
  finally{busy=false;if(editor?.open)renderEditor();draw();schedulePoll();}
}

// The header row (spec §8.4): the Custom soundtrack switch on the left; Reset, Import pack and Export pack
// (quiet) and Add songs… on the right. One song at a time is added from its row's Replace.
function discNow(){
  const on=!!status?.enabled,off=locked()||!status?.image_ok;
  const toggle=el('button',{type:'button',id:'music-enabled',class:'look-switch',role:'switch','aria-label':'Custom soundtrack','aria-checked':String(on),disabled:off,onclick:()=>act('music_set_enabled',{on:!on})});
  const nothingCustom=!slots().some(s=>s.source!=='original'||s.pending)&&status?.base?.kind!=='package';
  const buttons=el('div',{class:'music-toolbar-actions'},
    el('button',{type:'button',class:'btn quiet',text:'Reset','aria-label':'Reset soundtrack',disabled:off||nothingCustom,onclick:resetDialog}),
    el('button',{type:'button',class:'btn quiet',text:'Import pack',disabled:off,onclick:packDialog}),
    el('button',{type:'button',class:'btn quiet',text:'Export pack',disabled:off,onclick:shareDialog}),
    el('button',{type:'button',class:'btn',text:'Add songs…',disabled:off,onclick:bulkDialog}));
  return el('section',{class:'music-toolbar'},
    el('div',{class:'music-toggle'},toggle,el('label',{for:'music-enabled'},el('span',{class:'t',text:'Custom soundtrack'}),el('span',{class:'d',text:on?'Your selected songs play in-game.':'Your game’s default soundtrack plays. Custom songs are kept.'}))),
    buttons);
}
function statusLine(){
  if(working()){
    const p=status.progress||{},pct=p.total?Math.min(100,Math.max(0,Math.round((p.done||0)/p.total*100))):null;
    // Several songs at once say which one: "Adding song 2 of 5: name".
    const label=status.phase==='encoding'?(/^Adding song /.test(status.progress?.label||'')?status.progress.label:'Preparing your song'):'Applying your soundtrack';
    return el('div',{class:'glass music-progress',role:'status'},el('div',{},el('strong',{text:label}),el('span',{text:pct===null?'Please wait…':`${pct}%`})),el('progress',{max:'100',value:pct,'aria-label':label}),el('p',{class:'note',text:status.phase==='encoding'?'Your song will appear in the list when it is ready.':'Keep the launcher open until this finishes.'}));
  }
  if(status?.phase==='done'||status?.phase==='problem')return el('div',{class:'glass mz-done'+(status.phase==='problem'?' music-error':''),role:status.phase==='problem'?'alert':'status'},el('span',{text:status.phase==='done'?(slots().some(s=>s.pending)?'Changes saved. Apply them when you are ready.':'Your soundtrack is up to date.'):(status.note||'Something went wrong.')}),el('button',{type:'button',text:'Dismiss',onclick:()=>act('music_dismiss')}));
  return null;
}
// A row dragged by the mouse: after a few pixels of movement the row is
// lifted (a ghost follows the pointer, the row itself dims), the row under
// the pointer shows a line above or below it for where the song will land,
// the list scrolls when the pointer nears the top or bottom of the window,
// and letting go sends one music_move. Escape drops the drag.
function beginRowDrag(ev,row,from){
  const startX=ev.clientX,startY=ev.clientY;let lifted=false,ghost=null,target=null,scrollTimer=null;
  const rows=()=>Array.from(document.querySelectorAll('.mz-table tbody tr.mz-track'));
  const clearMarks=()=>{for(const r of rows())r.classList.remove('drop-above','drop-below');};
  const place=(x,y)=>{
    clearMarks();target=null;
    const under=document.elementFromPoint(x,y)?.closest?.('tr.mz-track');
    const list=rows();if(!list.length)return;
    let tr=under;
    if(!tr){const first=list[0].getBoundingClientRect(),lastRect=list[list.length-1].getBoundingClientRect();if(y<first.top)tr=list[0];else if(y>lastRect.bottom)tr=list[list.length-1];else return;}
    const slot=Number(tr.dataset.slot);const rect=tr.getBoundingClientRect();const below=y>rect.top+rect.height/2;
    // The list move: the song lands ON the marked slot (the ones between shift).
    let to=slot;
    if(from<slot&&!below)to=slot-1;
    if(from>slot&&below)to=slot+1;
    if(to===from||slot===from)return;
    target=to;tr.classList.add(below?'drop-below':'drop-above');
  };
  const autoScroll=y=>{
    const edge=48,h=innerHeight;let dy=0;
    if(y<edge)dy=-Math.ceil((edge-y)/4);else if(y>h-edge)dy=Math.ceil((y-(h-edge))/4);
    if(dy&&!scrollTimer)scrollTimer=setInterval(()=>{(document.scrollingElement||document.documentElement).scrollBy(0,dy*6);},16);
    if(!dy&&scrollTimer){clearInterval(scrollTimer);scrollTimer=null;}
  };
  const finish=commit=>{
    document.removeEventListener('pointermove',onMove);document.removeEventListener('pointerup',onUp);document.removeEventListener('pointercancel',onCancel);document.removeEventListener('keydown',onKey);
    if(scrollTimer){clearInterval(scrollTimer);scrollTimer=null;}
    ghost?.remove();row.classList.remove('dragging');clearMarks();document.body.classList.remove('mz-dragging');dragFrom=null;
    if(commit&&target!==null&&target!==from)act('music_move',{from,to:target});
  };
  const onMove=e=>{
    if(!lifted){if(Math.abs(e.clientX-startX)+Math.abs(e.clientY-startY)<5)return;lifted=true;dragFrom=from;row.classList.add('dragging');document.body.classList.add('mz-dragging');
      ghost=row.cloneNode(true);ghost.className='mz-ghost';const r=row.getBoundingClientRect();ghost.style.width=r.width+'px';document.body.append(ghost);}
    ghost.style.transform=`translate(${e.clientX+12}px,${e.clientY-14}px)`;
    place(e.clientX,e.clientY);autoScroll(e.clientY);
  };
  const onUp=()=>finish(lifted);
  const onCancel=()=>finish(false);
  const onKey=e=>{if(e.key==='Escape')finish(false);};
  document.addEventListener('pointermove',onMove);document.addEventListener('pointerup',onUp);document.addEventListener('pointercancel',onCancel);document.addEventListener('keydown',onKey);
}

function discList(){
  const list=slots();
  const sec=el('section',{class:'glass mz-disc',id:'disc'});
  const custom=list.filter(s=>s.source!=='original').length;
  sec.append(el('div',{class:'music-list-heading'},el('h2',{text:'Soundtrack'}),el('span',{text:`${list.length} songs${custom?` · ${custom} custom`:''}`})));
  if(!list.length){sec.append(el('div',{class:'music-empty'},el('h3',{text:status?.image_ok?'No songs found':'Set up your game first'}),el('p',{class:'note',text:status?.image_note||problem||'Your soundtrack will appear here once your game is ready.'}),el('button',{type:'button',text:'Try again',disabled:busy,onclick:refresh})));return sec;}
  const table=el('table',{class:'mz-table','aria-label':'Soundtrack songs'});
  table.append(el('thead',{},el('tr',{},el('th',{scope:'col',class:'mz-col-n',text:'#'}),el('th',{scope:'col',class:'mz-col-mv','aria-label':'Order'}),el('th',{scope:'col',text:'Song'}),el('th',{scope:'col',class:'mz-col-time',text:'Length'}),el('th',{scope:'col',class:'mz-col-actions',text:'Actions'}))));
  const body=el('tbody');
  const last=list.length?list[list.length-1].slot:0;
  for(const song of list){
    const pending=song.pending,original=pending?pending.kind==='original':song.source==='original';
    const o=song.original;
    // An original song the player renamed keeps its audio and shows their
    // name: the backend puts that name in song.title, so it wins over the
    // disc's own text. "Restore original" (a pending reset) shows the disc's.
    const renamed=!pending&&song.source==='original'&&!!o&&(song.title!==o.title||song.artist!==o.artist||song.album!==o.album);
    const title=original?((pending?'':song.title)||o?.title||slotTitle({...song,title:'',original:null,source:'original'})):(pending?.title||song.title||'Untitled');
    const artist=original?((pending?'':song.artist)||o?.artist||''):((pending?.artist??song.artist)||'');
    // Actions (spec §8.4): a pending song offers Undo only; a custom (or renamed) song, Restore
    // original and Replace; the disc's own song, Replace. They wrap in a narrow window, never clip.
    const acts=el('div',{class:'mz-acts'});
    if(pending)acts.append(el('button',{type:'button',class:'btn sm quiet',text:'Undo','aria-label':`Undo changes to song ${song.slot}`,disabled:locked(),onclick:()=>act('music_unstage',{slot:song.slot})}));
    else{
      if(song.source!=='original'||renamed)acts.append(el('button',{type:'button',class:'btn sm quiet',text:'Restore original','aria-label':`Restore original song ${song.slot}`,disabled:locked(),onclick:()=>act('music_stage_stock',{slot:song.slot})}));
      acts.append(el('button',{type:'button',class:'btn sm',text:'Replace','aria-label':`Replace song ${song.slot}`,disabled:locked(),onclick:()=>chooseFile(song.slot)}));
    }
    const actions=el('td',{class:'mz-col-actions'},acts);
    const name=el('td',{class:'mz-col-what'},el('button',{type:'button',class:'music-song-name',text:title,'aria-label':`Edit details for song ${song.slot}: ${title}`,disabled:locked(),onclick:()=>editDetails(song.slot)}),el('span',{class:'mz-src'},...sourceLine(song,artist)));
    // The order is the player's to change: drag a row onto another (the
    // songs between shift by one), or use the arrows, which a pad or the
    // keyboard can reach. Both go through music_move.
    const arrows=el('span',{class:'mz-move'},
      el('button',{type:'button',class:'mz-arrow',text:'▲','aria-label':`Move song ${song.slot} up`,disabled:locked()||song.slot<=1,onclick:()=>act('music_move',{from:song.slot,to:song.slot-1})}),
      el('button',{type:'button',class:'mz-arrow',text:'▼','aria-label':`Move song ${song.slot} down`,disabled:locked()||song.slot>=last,onclick:()=>act('music_move',{from:song.slot,to:song.slot+1})}));
    const row=el('tr',{class:`mz-track${pending?' staged':''}${!pending&&song.source!=='original'?' custom':''}`,'data-slot':String(song.slot)});
    // Plain pointer events, not HTML5 drag and drop: the launcher's window
    // never delivers dragstart (Tauri owns OS drag and drop on Windows), so
    // the row follows the mouse itself. Press anywhere on the row that is
    // not a control, move, and let go over the place it should take.
    row.addEventListener('pointerdown',ev=>{if(ev.button!==0||locked()||ev.target.closest('button,select,input,a'))return;beginRowDrag(ev,row,song.slot);});
    row.append(el('td',{class:'mz-col-n'},el('span',{class:'mz-n',text:String(song.slot).padStart(2,'0')})),el('td',{class:'mz-col-mv'},arrows),name,el('td',{class:'mz-col-time',text:pending?'':original?(mmss(song.duration_ms??o?.duration_ms)||''):(mmss(song.duration_ms)||'')}),actions);
    body.append(row);
  }
  table.append(body);sec.append(table);return sec;
}
function draw(){
  const screen=document.getElementById('screen'),parts=[discNow()];
  if(problem)parts.push(el('p',{class:'music-error',role:'alert',text:problem}));
  if(notice)parts.push(el('p',{class:'note',role:'status',text:notice}));
  if(status&&!status.image_ok&&slots().length)parts.push(el('p',{class:'music-error',role:'status',text:status.image_note||'Set up your game before editing its soundtrack.'}));
  const progress=statusLine();if(progress)parts.push(progress);
  parts.push(discList());
  parts.push(el('p',{class:'music-footnote',text:'Custom music applies to offline play. Online uses the shared Reborn soundtrack.'}));
  const focus=document.activeElement?.getAttribute?.('aria-label');
  screen.replaceChildren(...parts);
  if(focus&&!editor?.open)Array.from(screen.querySelectorAll('[aria-label]')).find(n=>n.getAttribute('aria-label')===focus)?.focus({preventScroll:true});
  // The bar lists every pending song. Apply waits while the game image is missing or the core is
  // working (a second Apply would start a second rebuild); Discard waits only while the core works.
  bar.update(pendingChanges());
  bar.lock(locked()||!status?.image_ok,locked());
  applyOutcome();
  return parts;
}

// ---------------------------------------------------------------------------
// Pack dialogs (pack-level operations keep a dialog)
// ---------------------------------------------------------------------------

function openDialog(label) {
  document.querySelector(".mod-dialog")?.remove();
  const dialog = el("dialog", { class: "mod-dialog music-dialog", "aria-label": label });
  dialog.addEventListener("close", () => dialog.remove());
  return dialog;
}

function showDialog(dialog) {
  document.body.append(dialog);
  if (typeof dialog.showModal === "function") dialog.showModal();
}

function resetDialog() {
  const custom=slots().filter(s=>s.source!=='original').length;
  const base=status?.base?.kind==='package'?status.base.name:null;
  const dialog=openDialog('Reset soundtrack');
  const body=el('div',{class:'mod-dialog-body'});
  const lines=el('ul',{class:'mz-preview'});
  if(custom)lines.append(el('li',{text:`${custom} custom song${custom===1?'':'s'} go back to the song your game shipped with.`}));
  if(base)lines.append(el('li',{text:`The ${base} is switched off.`}));
  lines.append(el('li',{text:'Your song files and packs are kept, so you can add them again without converting.'}));
  body.append(
    el('h2',{text:'Reset the soundtrack?'}),
    el('p',{text:'Every song goes back to the one your game shipped with. This rebuilds the game once, about half a minute.'}),
    lines,
    el('div',{class:'store-dialog-actions'},
      el('button',{type:'button',text:'Cancel',onclick:()=>dialog.close()}),
      el('button',{type:'button',class:'primary',text:'Reset soundtrack',disabled:locked(),onclick:()=>{dialog.close();act('music_reset_all',{confirm:true});}})));
  dialog.append(body);showDialog(dialog);
}
async function packDialog() {
  const path = await pickPath("pack");
  if (!path) return;
  let preview;
  try {
    preview = await call("music_pack_preview", { path });
  } catch (e) {
    problem = reason(e);
    draw();
    return;
  }
  const dialog = openDialog("Import a pack");
  const body = el("div", { class: "mod-dialog-body" });
  const rows = el("ul", { class: "mz-preview" });
  for (const song of preview.songs || []) {
    rows.append(el("li", { text: `Slot ${song.slot}: ${[song.title, song.artist].filter(Boolean).join(" by ")} ${mmss(song.duration_ms) ? `(${mmss(song.duration_ms)})` : ""} replaces ${song.replaces}` }));
  }
  body.append(
    el("span", { class: "store-kicker", text: "Soundtrack pack" }),
    el("h2", { text: preview.name || "Pack" }),
    el("p", { class: "note", text: [preview.author ? `by ${preview.author}` : null, `${(preview.songs || []).length} song(s)`, preview.format === 1 ? "older pack format" : null].filter(Boolean).join(" · ") }),
    preview.description ? el("p", { text: preview.description }) : null,
    rows,
    el("div", { class: "store-dialog-actions" },
      el("button", { type: "button", text: "Cancel", onclick: () => dialog.close() }),
      el("button", { type: "button", class: "primary", text: "Import songs", disabled: locked(), onclick: async () => { dialog.close(); await act("music_stage_pack", { path }); } }),
    ),
  );
  dialog.append(body);
  showDialog(dialog);
}

// Several songs at once: pick the files, choose where they start, and the
// core adds them in file order, one slot each, with names from their tags
// (and the online lookup for what the tags lack). Each can be edited
// afterwards like any other song.
async function bulkDialog() {
  if (locked()) return;
  let paths = [];
  try {
    paths = await call("pick_paths", { kind: "audio" });
  } catch (e) {
    problem = reason(e);
    draw();
    return;
  }
  if (!Array.isArray(paths) || !paths.length) return;
  // Numbered files ("01 ...", "02 ...") land in their numbered order.
  paths = [...paths].sort((a, b) => a.split(/[\\/]/).pop().localeCompare(b.split(/[\\/]/).pop(), undefined, { numeric: true, sensitivity: "base" }));
  const count = paths.length;
  const dialog = openDialog("Add songs");
  const body = el("div", { class: "mod-dialog-body" });
  const list = el("ol", { class: "mz-preview" });
  for (const p of paths) list.append(el("li", { text: p.split(/[\\/]/).pop() }));
  const start = el("select", { id: "music-start-slot", "aria-label": "Start at song" });
  const first = firstFreeSlot();
  for (const s of slots()) start.append(el("option", { value: String(s.slot), text: `${String(s.slot).padStart(2, "0")} · ${s.pending ? pendingWhat(s.pending, s) : slotTitle(s)}`, selected: s.slot === first }));
  const note = el("p", { class: "note", role: "status" });
  const update = () => {
    const from = Number(start.value), room = Math.max(0, slots().length - from + 1);
    note.textContent = count > room
      ? `${room} of these fit from song ${from} on; the last ${count - room} would be left out. Start lower to fit more.`
      : `They take songs ${from} to ${from + count - 1}, in this order, replacing what is there. Names come from the files and can be edited afterwards.`;
  };
  start.addEventListener("change", update);
  update();
  body.append(
    el("span", { class: "store-kicker", text: "Add songs" }),
    el("h2", { text: `${count} song${count === 1 ? "" : "s"}` }),
    list,
    el("label", { class: "mz-field" }, el("span", { text: "Start at song" }), start),
    note,
    el("div", { class: "store-dialog-actions" },
      el("button", { type: "button", text: "Cancel", onclick: () => dialog.close() }),
      el("button", { type: "button", class: "primary", text: `Add ${count} song${count === 1 ? "" : "s"}`, disabled: locked(), onclick: async () => { dialog.close(); await act("music_stage_files", { paths, startSlot: Number(start.value) }); } }),
    ),
  );
  dialog.append(body);
  showDialog(dialog);
}

function shareDialog() {
  const dialog = openDialog("Share a pack");
  const body = el("div", { class: "mod-dialog-body" });
  const boxes = [];
  const list = el("div", { class: "mz-share-list" });
  for (const s of slots()) {
    const box = el("input", { type: "checkbox", checked: s.source !== "original" });
    boxes.push([s.slot, box]);
    list.append(el("label", { class: "mz-share-row" }, box, el("span", { text: `Slot ${s.slot}: ${slotLine(s)}` })));
  }
  const name = el("input", { type: "text", placeholder: "Pack name" });
  const author = el("input", { type: "text", placeholder: "Your name" });
  const description = el("input", { type: "text", placeholder: "A line about it (optional)" });
  body.append(
    el("span", { class: "store-kicker", text: "Share" }),
    el("h2", { text: "Save a soundtrack pack" }),
    el("p", { text: "Choose songs to share. Apply any pending changes before exporting." }),
    list,
    el("label", { class: "mz-field" }, el("span", { text: "Name" }), name),
    el("label", { class: "mz-field" }, el("span", { text: "Author" }), author),
    el("label", { class: "mz-field" }, el("span", { text: "Description" }), description),
    el("div", { class: "store-dialog-actions" },
      el("button", { type: "button", text: "Cancel", onclick: () => dialog.close() }),
      el("button", { type: "button", class: "primary", text: "Save pack", onclick: async () => {
        const chosen = boxes.filter(([, b]) => b.checked).map(([n]) => n);
        if (!chosen.length) { let warning=body.querySelector("[role=alert]");if(!warning){warning=el("p",{role:"alert",class:"music-error"});body.append(warning);}warning.textContent="Choose at least one song to export."; return; }
        const path = await pickPath("save_pack");
        if (!path) return;
        dialog.close();
        busy = true; problem = null; notice = null; draw();
        try {
          const saved = await call("music_export", { path, slots: chosen, name: name.value.trim(), author: author.value.trim(), description: description.value.trim() });
          notice = `Saved ${saved}.`;
        } catch (e) {
          problem = reason(e);
        }
        busy = false;
        await load();
        draw();
      } }),
    ),
  );
  dialog.append(body);
  showDialog(dialog);
}

// The one save bar (spec §7, §8.4): the songs waiting to be applied, then Discard and Apply changes,
// in its own lane under the page. The core owns the pending list; Apply rebuilds the disc once.
const bar = mountSaveBar(document.getElementById("save-lane"), {
  noun: "pending change",
  saveText: "Apply changes",
  onSave: () => act("music_apply", { confirm: true }, { fromBar: true }),
  onDiscard: () => act("music_unstage", { slot: null }, { fromBar: true }),
  returnFocus: () => document.querySelector("#disc .mz-track button:not(:disabled)"),
});

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

const clearButton = document.getElementById("clear");
if (clearButton) {
  clearButton.addEventListener("click", () => {
    document.getElementById("out").textContent = "";
  });
}

for (const a of typeof document.querySelectorAll === "function" ? document.querySelectorAll(".shell-nav a") : []) {
  if ((a.getAttribute("href") || "").toLowerCase() === "music.html") a.classList.add("current");
}

if (bridge) {
  refresh();
} else {
  problem = "The launcher core is not attached. Open this page inside the launcher window.";
  draw();
}
