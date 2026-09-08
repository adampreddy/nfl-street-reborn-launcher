// Installed mod collection. The core owns package conflicts and activation.
const bridge = window.__TAURI__ ? window.__TAURI__.core : null;

// The emulator the Settings page names, if any. Passed with every action
// that links the texture folder, so the mods land in the user folder a
// launch of that emulator will actually read.
function runtimeHint() {
  try {
    return localStorage.getItem("h:runtime") || null;
  } catch (_) {
    return null;
  }
}

// What the last reads returned. Null until read, and null again if a read
// failed, so a stale list is never drawn as current.
let library = null; // mod_packages
let options = null; // controls_options, or { error }
let current = null; // controls_current
// The last sync the page saw, from mods_sync, mods_toggle or controls_set.
let lastSync = null;
// One action at a time. A second click while the first is composing would
// race it on the same tree.
let busy = false;
// A failure, in the core's words, drawn at the top. Cleared by the next action.
let problem = null;
// A partial answer. Not a failure: "recorded, applied at the next launch".
let notice = null;

// ---------------------------------------------------------------------------
// Talking to the core
// ---------------------------------------------------------------------------

function log(what, value) {
  const pre = document.getElementById("out");
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

// Read everything the page shows. Each read settles on its own, so a failed
// pad scan does not blank the mod list and a failed mod list does not hide the
// button art. What failed is said where it would have been drawn.
async function load() {
  const [lib, opts, cur] = await Promise.allSettled([
    call("mod_packages"),
    call("controls_options", { runtime: null }),
    call("controls_current"),
  ]);
  library = lib.status === "fulfilled" ? lib.value : null;
  if (lib.status === "rejected") problem = `The mod library could not be read: ${reason(lib.reason)}`;
  options = opts.status === "fulfilled" ? opts.value : { error: reason(opts.reason) };
  current = cur.status === "fulfilled" ? cur.value : null;
  if (cur.status === "rejected") notice = `Which button art is in force could not be read: ${reason(cur.reason)}`;
}

// One action, then a fresh read, then a redraw. The result of every action is
// a sync view or carries one, and that is what "what the game loads" shows.
async function act(cmd, args) {
  if (busy) return;
  busy = true;
  problem = null;
  notice = null;
  if (window.setLauncherBusy) window.setLauncherBusy(true); else draw();
  try {
    const r = await call(cmd, args);
    if (cmd === "controls_set") {
      if (r && r.synced) lastSync = r.synced;
      else if (r) notice = r.sync_note;
    } else {
      lastSync = r;
      if(r?.synced===false)notice=r.note||"Your selection is saved, but the mod files could not be applied yet.";
    }
    await load();
  } catch (e) {
    problem = reason(e);
  }
  busy = false;
  window.setLauncherBusy?.(false);
  draw();
}

async function refresh() {
  problem = null;
  notice = null;
  try {
    await load();
  } catch (e) {
    problem = reason(e);
  }
  draw();
}

// ---------------------------------------------------------------------------
// Building the screen
// ---------------------------------------------------------------------------

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined) continue;
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else if (k === "onclick") node.addEventListener("click", v);
    else if (k === "onchange") node.addEventListener("change", v);
    else if (k === "checked" || k === "disabled") node[k] = !!v;
    else node.setAttribute(k, v);
  }
  for (const c of children) {
    if (c !== null && c !== undefined) node.append(c);
  }
  return node;
}

function mb(bytes) {
  if (typeof bytes !== "number") return null;
  return `${(bytes / 1048576).toFixed(1)} MB`;
}

function packages() {
  return library && Array.isArray(library.packages) ? library.packages : [];
}

function managed(id) {
  const list = library && Array.isArray(library.managed_by_controls) ? library.managed_by_controls : [];
  return list.includes(id);
}

function staleSection() {
  const listed = library && Array.isArray(library.activation) ? library.activation : [];
  const stale = listed.filter((a) => !packages().some((p) => p.id === a.id && p.version === a.version));
  if (stale.length === 0) return null;
  const sec = el("section", { class: "block" }, el("h2", { text: "Missing mod files" }));
  sec.append(
    el("p", {
      class: "note",
      text:
        "The launcher leaves these on. Reinstalling the package brings it back " +
        "without another click; turning it off here is the other way out.",
    }),
  );
  for (const a of stale) {
    sec.append(
      el(
        "div",
        { class: "mod" },
        el("span", { class: "what", text: `${a.id}@${a.version}` }),
        el("button", {
          type: "button",
          text: "Turn off",
          disabled: busy,
          onclick: () => act("mods_toggle", { id: a.id, version: a.version, on: false, runtime: runtimeHint() }),
        }),
      ),
    );
  }
  return sec;
}

function unreadableSection() {
  const list = library && Array.isArray(library.unreadable) ? library.unreadable : [];
  if (list.length === 0) return null;
  const sec = el("section", { class: "block" }, el("h2", { text: "Unavailable mods" }));
  for (const why of list) sec.append(el("p", { class: "partial", text: String(why) }));
  return sec;
}

let query = '', category = 'Footballs', selectedMod = null;
const categoryOf = p => /football/i.test(p.name+' '+p.id)?'Footballs':/soundtrack|music|audio/i.test(p.name+' '+p.id)?'Soundtrack':null;
const colors = ['#514941','#3f4c49','#484357','#49505b','#59464a','#514f3e'];
function storeCard(p, index) {
  const card=el('article',{class:'store-card'});
  const open=el('button',{type:'button',class:'store-open','aria-label':p.name||p.id,onclick:()=>showMod(p)});
  const art=el('span',{class:'store-art','aria-hidden':'true'});art.style.backgroundColor=colors[index%colors.length];
  open.append(art,el('span',{class:'store-card-body'},el('strong',{text:p.name||p.id}),el('small',{text:categoryOf(p)+' · '+(p.version?'Version '+p.version:'Installed')})));
  card.append(open,el('div',{class:'store-card-actions'},el('span',{class:p.active?'store-state active':'store-state',text:p.active?'Enabled':'Installed'}),el('button',{type:'button',text:p.active?'Disable':'Enable','aria-label':(p.active?'Disable ':'Enable ')+(p.name||p.id),disabled:busy,onclick:()=>act('mods_toggle',{id:p.id,version:p.version,on:!p.active,runtime:runtimeHint()})})));
  return card;
}
function showMod(p) {
  document.querySelector('.mod-dialog')?.remove();
  const dialog=el('dialog',{class:'mod-dialog','aria-label':p.name||p.id});
  const art=el('div',{class:'store-art','aria-hidden':'true'});art.style.backgroundColor=colors[Math.max(0,packages().indexOf(p))%colors.length];
  const conflict=(library?.exclusive||[]).find(g=>(g.members||[]).includes(p.id));
  dialog.append(art,el('div',{class:'mod-dialog-body'},el('span',{class:'store-kicker',text:categoryOf(p)}),el('h2',{text:p.name||p.id}),el('p',{text:p.description||'Customize your game with this installed Reborn mod.'}),el('p',{class:'note',text:['Version '+p.version,mb(p.bytes),typeof p.files==='number'?p.files+' files':null].filter(Boolean).join(' · ')}),conflict?el('p',{class:'note',text:'Enabling this mod replaces other enabled mods that change the same files.'}):null,el('div',{class:'store-dialog-actions'},el('button',{type:'button',text:'Close',onclick:()=>dialog.close()}),el('button',{type:'button',class:'primary',text:p.active?'Disable mod':'Enable mod',disabled:busy,onclick:()=>{dialog.close();act('mods_toggle',{id:p.id,version:p.version,on:!p.active,runtime:runtimeHint()});}}))));
  dialog.addEventListener('close',()=>dialog.remove());document.body.append(dialog);dialog.showModal();
}
function draw() {
  const screen=document.getElementById('screen'),parts=[];
  if(problem)parts.push(el('p',{class:'refusal',role:'alert',text:problem}));
  if(notice)parts.push(el('p',{class:'partial',role:'status',text:notice}));
  const all=packages().filter(p=>!managed(p.id));
  const toolbar=el('div',{class:'store-toolbar'}),search=el('input',{type:'search',placeholder:'Search your mods','aria-label':'Search mods',value:query});
  const results=el('section',{class:'store-grid','aria-label':'Mods'}),count=el('p',{class:'store-count',role:'status'});
  const renderResults=()=>{const list=all.filter(p=>categoryOf(p)===category&&((p.name||'')+' '+p.id).toLowerCase().includes(query.toLowerCase()));count.textContent=list.length+' '+(list.length===1?'mod':'mods');results.replaceChildren(...list.map((p)=>storeCard(p,all.indexOf(p))));if(!list.length)results.append(el('div',{class:'store-empty'},el('h2',{text:all.length?'No matching mods':'Your mod library starts here'}),el('p',{text:all.length?'Try another search or category.':'Import your existing installation to bring your mods with you.'}),!all.length?el('a',{href:'onboarding.html',class:'button',text:'Import mods'}):null));};
  search.addEventListener('input',()=>{query=search.value;renderResults();});
  toolbar.append(search,el('a',{href:'onboarding.html',class:'button',text:'Import mods'}),el('button',{type:'button',text:'Refresh',disabled:busy,onclick:refresh}));parts.push(toolbar);
  const tabs=el('div',{class:'section-tabs','aria-label':'Mod categories'});for(const name of ['Footballs','Soundtrack'])tabs.append(el('button',{type:'button',text:name,'aria-pressed':String(name===category),onclick:()=>{category=name;tabs.querySelectorAll('button').forEach(b=>b.setAttribute('aria-pressed',String(b.textContent===category)));renderResults();}}));parts.push(tabs);
  parts.push(count,results);renderResults();
  const stale=staleSection(),unreadable=unreadableSection();if(stale)parts.push(stale);if(unreadable)parts.push(unreadable);
  parts.push(el('p',{class:'note store-footnote',text:'Mods are applied automatically when you start the game. Button textures are in Controllers.'}));
  screen.replaceChildren(...parts);return parts;
}

// ---------------------------------------------------------------------------
// Boot
// ---------------------------------------------------------------------------

const clearButton = document.getElementById("clear");
if (clearButton) {
  clearButton.addEventListener("click", () => {
    document.getElementById("out").textContent = "";
  });
}

// A harness fakes document without querySelectorAll; the nav is not its concern.
for (const a of typeof document.querySelectorAll === "function" ? document.querySelectorAll(".shell-nav a") : []) {
  if ((a.getAttribute("href") || "").toLowerCase() === "mods.html") a.classList.add("current");
}

if (bridge) {
  refresh();
} else {
  problem = "The launcher core is not attached. Open this page inside the launcher window.";
  draw();
}
