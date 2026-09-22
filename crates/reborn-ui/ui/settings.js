// Settings, as a page: where things live, and the few paths a person can set.

import { bridge, call, cleanPath, el, fact, facts, get, markCurrent, partial, reason, refusal, set, wireCoreFold } from "./shell.js";

const state = {};
// What changed in each release, on the site. Opened in the browser.
const PATCH_NOTES_URL = "https://nflstreetreborn.com/blog";
const tabForHash = () => ({general:"General",graphics:"Graphics",display:"Display",audio:"Audio",saves:"Saves",updates:"Updates",files:"Files & storage"})[location.hash?.slice(1).toLowerCase()] || "General";
let settingsTab=tabForHash();
window.addEventListener?.("hashchange", () => { if (!busy) { settingsTab=tabForHash(); draw(); } });
let problem = null;
let busy = false;
// The update flow's last answers, shown in place.
let update = null; // update_check view
let downloaded = null; // update_download view
let leagueSaid = null;
// The Tier 2 choice, while it is open. Set from a settings_set_path answer
// that stored nothing because the chosen image is NFL Street 2 but not a
// Reborn build. The words come from the backend so the launch refusal, the
// Play badge and this card all describe one disc. Never "custom": this is a
// fault to repair, not a preference anybody expressed.
let unverified = null;

function ok(x) {
  return x && !x.error;
}

async function load() {
  const reads = {
    dolphin: call("dolphin_preferences", {runtime:get("runtime")}),
    graphics: call("setup_graphics_options"),
    hardware: call("setup_hardware_info"),
    progress: call("onboarding_progress"),
    preferences: call("settings_paths"),
    installation: call("setup_install_info"),
    paths: call("paths"),
    check: call("launch_check", { runtime: get("runtime"), game: get("game"), userDir: get("userDir") }),
    identity: call("identity_status"),
    policy: call("update_policy"),
    queue: call("stats_queue"),
    content: call("content_status"),
  };
  const keys = Object.keys(reads);
  const settled = await Promise.allSettled(Object.values(reads));
  keys.forEach((k, i) => {
    state[k] = settled[i].status === "fulfilled" ? settled[i].value : { error: reason(settled[i].reason) };
  });
}

// The boxes. Saved as typed, quotes stripped, and the Play page re-checks on
// its next load. Blank means "find it", which is the normal state.
const BOXES = [
  {
    key: "game",
    setting: "game",
    name: "Game image",
    hint: "Your game image. Leave empty to find it automatically.",
  },
  {
    key: "legacyRoot",
    setting: "legacy_root",
    name: "Existing install",
    hint: "Your earlier Reborn installation, containing the User and MODS folders.",
  },
  {
    key: "runtime",
    setting: "runtime",
    name: "Emulator",
    hint: "The bundled Reborn Dolphin is selected automatically.",
  },
  {
    key: "userDir",
    setting: "user_dir",
    name: "User folder",
    hint: "Contains your saves and Dolphin settings. Leave empty to use the detected folder.",
  },
];

function boxesBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Game & installation" }));
  sec.append(el("p", { class: "note", text: "Locations are detected automatically. Choose an override only if you want to use a different installation." }));
  for (const b of BOXES) {
    const value = ok(state.preferences) ? state.preferences[b.key] : get(b.key);
    const input = el("input", { id: `path-${b.key}`, type: "text", class: "wide", value: value || "", placeholder: "Detect automatically", disabled: busy });
    const save = async () => {
      const v = cleanPath(input.value);
      await act(`Save ${b.name}`, "settings_set_path", { which: b.setting, value: v }, (r) => {
        // Nothing was stored. The backend will not record the escape hatch on
        // the player's behalf, so the choice is put to them here and the path
        // is saved only if they take it.
        if (r && r.unverified) { unverified = { ...r.unverified, which: b.setting, key: b.key, value: v }; return; }
        unverified = null;
        set(b.key, v);
      });
    };
    const browse = async () => {
      if (busy) return;
      try {
        const path = await call("pick_path", { kind: b.key === "game" ? "game" : b.key === "runtime" ? "runtime" : "folder" });
        if (path) { input.value = path; await save(); }
      } catch (e) { problem = reason(e); draw(); }
    };
    const row = el("div", { class: "path-entry" }, input,
      el("button", { type: "button", text: "Browse", "aria-label": `Browse for ${b.name}`, disabled: busy, onclick: browse }),
      el("button", { type: "button", text: "Save", "aria-label": `Save ${b.name}`, disabled: busy, onclick: save }));
    sec.append(el("div", { class: "settings-row stack" }, el("label", { for: `path-${b.key}`, text: b.name }), row, el("span", { class: "hint", text: b.hint })));
  }
  sec.append(el("a", { href: "onboarding.html", class: "button", text: "Resume game setup" }));
  return sec;
}

// The repair every refusal points at.
//
// The Play card's badge and the launch refusal both send the player here, and
// until now "here" was a path box and a Browse button: no way out for somebody
// whose disc could not be converted. This is the same download the setup
// wizard offers, on the page they are actually sent to.
async function repairGame() {
  await act("Repair my game", "setup_download_reborn_game", {}, () => { unverified = null; });
}

// "Play it anyway": the escape hatch, taken deliberately, for this one image.
// This is the ONLY call in the launcher that sets acknowledgeUnverified.
async function playItAnyway(choice) {
  await act(choice.play_anyway, "settings_set_path", { which: choice.which, value: choice.value, acknowledgeUnverified: true }, () => {
    unverified = null;
    set(choice.key, choice.value);
  });
}

// The Tier 2 choice, drawn as a modal when the browser has one and as a card
// on the page when it does not. Repair is primary; the escape hatch is not.
function unverifiedChoice() {
  const choice = unverified;
  if (!choice) return null;
  const box = el("dialog", { id: "unverified-game", class: "settings-choice", role: "alertdialog", "aria-labelledby": "unverified-game-title" },
    el("h2", { id: "unverified-game-title", text: choice.headline }),
    el("p", { text: choice.detail }),
    el("div", { class: "settings-actions" },
      el("button", { type: "button", class: "primary", text: choice.repair, disabled: busy, onclick: () => repairGame() }),
      el("button", { type: "button", text: choice.play_anyway, disabled: busy, onclick: () => playItAnyway(choice) }),
      el("button", { type: "button", text: "Cancel", disabled: busy, onclick: () => { unverified = null; draw(); } })));
  return box;
}

// The standing repair, for a player whose saved game file is already an
// unconverted disc. Same words, same primary action, on the page both the Play
// badge and the launch refusal send them to.
function repairBlock() {
  if (!ok(state.check) || state.check.game_verdict !== "unconverted") return null;
  return el("section", { class: "block", id: "repair-game" },
    el("h2", { text: "Your game file isn't NFL Street Reborn." }),
    el("p", { text: "This is NFL Street 2, but not a Reborn build, so rosters, faces and stats won't match. This usually means setup couldn't convert your disc." }),
    el("div", { class: "settings-actions" },
      el("button", { type: "button", class: "primary", text: "Repair my game", disabled: busy, onclick: () => repairGame() })));
}

function foundBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Detected files" }));
  const c = state.check;
  if (!ok(c)) {
    sec.append(partial(`Could not check: ${c ? c.error : "not loaded"}`));
    return sec;
  }
  if (!c.ready) {
    sec.append(refusal(c.problem || "Your installation needs attention before you can play."));
    return sec;
  }
  const release=state.installation?.installed_release?.release;
  sec.append(facts([...(release?[fact("Previous release", release)]:[]),fact("Emulator", c.runtime),fact("Game image", c.game)]));
  return sec;
}

function whereBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Storage" }));
  const p = state.paths;
  if (!ok(p)) {
    sec.append(partial(`Could not read: ${p ? p.error : "not loaded"}`));
    return sec;
  }
  sec.append(
    facts([
      fact("Channel", p.channel_note || p.channel),
      fact("Program files", p.install_root),
      fact("Your data", p.data_root),
      fact("Save data", p.saves_survive_uninstall ? "Kept separately from the application." : "Stored inside the application folder."),
    ]),
  );
  const dirs = Array.isArray(p.directories) ? p.directories : [];
  const present = dirs.filter((d) => d.exists).map((d) => d.relative);
  sec.append(el("p", { class: "note", text: present.length ? `Folders in use under your data: ${present.join(", ")}.` : "No data folders exist yet." }));
  return sec;
}

async function act(label, cmd, args, after) {
  if (busy) return;
  const focusedId=document.activeElement?.id;
  busy = true;
  problem = null;
  if (window.setLauncherBusy) window.setLauncherBusy(true); else draw();
  const messages={update_check:'Checking for updates…',update_download:'Downloading and verifying…',update_apply:'Starting installer…',content_check:'Checking game files…',content_fetch:'Downloading game content…',content_promote:'Installing game content…'};
  if(messages[cmd])document.getElementById('screen').append(el('p',{class:'settings-status',role:'status',text:messages[cmd]}));
  try {
    const r = await call(cmd, args);
    after(r);
  } catch (e) {
    problem = `${label}: ${reason(e)}`;
  }
  await load();
  busy = false;
  window.setLauncherBusy?.(false);
  draw();
  if(focusedId)document.getElementById(focusedId)?.focus({preventScroll:true});
}

const mb = (b) => Math.max(1, Math.round((b || 0) / 1048576));

// Textures, the emulator, mods, game patches and the roster: the content
// system. The Play page checks on start and installs when the game is
// closed; this is where a person looks, checks, repairs, or installs now.
function preferenceRow(label, description, control, id) {
  return el('div', {class:'preference-row'},
    el('div', {class:'preference-description'},
      el(id ? 'label' : 'span', {class:'preference-label', ...(id ? {for:id} : {}), text:label}),
      description ? el('p', {text:description}) : null),
    el('div', {class:'preference-value settings-actions'}, control));
}

function contentBlock() {
  const sec=el('section',{class:'preference-group',id:'settings-game-content'},el('h2',{text:'Game content'}));
  const c=state.content;
  if(!ok(c)){sec.append(el('p',{class:'partial',text:c?.error||'Content status is unavailable. Refresh settings to try again.'}));return sec;}
  const working=['checking','fetching','installing'].includes(c.phase);
  const labels={checking:'Checking for updates',fetching:'Downloading content',installing:'Installing content',available:'Update available',staged:'Ready to install',installed:'Content updated',problem:'Needs attention'};
  const actions=el('div',{class:'settings-actions'});
  if(c.phase==='available')actions.append(el('button',{type:'button',class:'primary',text:`Download ${mb(c.download_bytes)} MB`,disabled:busy,onclick:()=>act('Download content','content_fetch',{},()=>{})}));
  else if(c.phase==='staged')actions.append(el('button',{type:'button',class:'primary',text:'Install now',disabled:busy,onclick:()=>act('Install content','content_promote',{confirm:true},()=>{})}));
  else if(c.phase==='installed')actions.append(el('button',{type:'button',text:'Dismiss summary',disabled:busy,onclick:()=>act('Dismiss','content_dismiss',{},()=>{})}));
  else actions.append(el('button',{type:'button',text:working?'Working…':'Check game content',disabled:busy||working,onclick:()=>act('Check content','content_check',{depth:null},()=>{})}));
  sec.append(preferenceRow(labels[c.phase]||'Game content updates',c.note||'Keep your textures, game patches, roster, and Dolphin up to date.',actions));
  if(working)sec.append(el('progress',{class:'settings-progress','aria-label':labels[c.phase],...(c.phase==='fetching'&&c.download_bytes?{value:String(Math.min(c.fetched_bytes||0,c.download_bytes)),max:String(c.download_bytes)}:{})}));
  const details=(c.components||[]).filter(x=>x&&['available','refused','problem'].includes(x.state));
  if(details.length){
    const list=el('ul',{class:'settings-update-details'});
    for(const comp of details)list.append(el('li',{text:comp.state==='available'?`${comp.component}: ${comp.repair?'repair':`${comp.installed?comp.installed+' → ':''}${comp.offered}`}, ${mb(comp.bytes)} MB`:`${comp.component}: ${comp.message||comp.state}`}));
    sec.append(list);
  }
  sec.append(preferenceRow('Verify game files','Check installed content and prepare repairs for missing or changed files.',el('button',{type:'button',text:'Verify and repair',disabled:busy||working,onclick:()=>act('Verify content','content_check',{depth:'verify'},()=>{})})));
  return sec;
}

function updatesBlock() {
  const wrap=el('div',{class:'settings-content'});
  const preferences=el('section',{class:'preference-group'},el('h2',{text:'Update preferences'}));
  if(ok(state.policy)){
    const policy=el('select',{id:'update-policy',disabled:busy},
      el('option',{value:'automatic',text:'Keep me up to date'}),el('option',{value:'ask',text:'Ask me first'}));
    policy.value=state.policy.automatic?'automatic':'ask';
    policy.addEventListener('change',()=>act('Update preference','update_policy_set',{automatic:policy.value==='automatic'},()=>{}));
    const row=preferenceRow('Automatic updates',state.policy.automatic?'Download and install verified updates automatically. See a summary when installation is complete.':'Nothing is downloaded until you say so.',policy,'update-policy');
    if(state.policy.automatic)row.querySelector('.preference-description label').append(el('span',{class:'settings-badge',text:'Recommended'}));
    preferences.append(row);
  }else preferences.append(el('p',{class:'partial',text:state.policy?.error||'Your update preference is unavailable.'}));
  const launcher=el('section',{class:'preference-group'},el('h2',{text:'Launcher'}));
  const actions=el('div',{class:'settings-actions'});
  actions.append(el('button',{type:'button',text:'Check for updates',disabled:busy,onclick:()=>act('Check for updates','update_check',{source:null},r=>{update=r;downloaded=null;})}));
  if(downloaded?.path)actions.append(el('button',{type:'button',class:'primary',text:'Install and restart',disabled:busy,onclick:()=>act('Install update','update_apply',{path:downloaded.path,confirm:true},r=>{update={...update,note:r.note};})}));
  else if(update?.state==='available')actions.append(el('button',{type:'button',class:'primary',text:`Download ${update.latest}`,disabled:busy,onclick:()=>act('Download update','update_download',{source:null},r=>{downloaded=r;if(r.check)update=r.check;})}));
  launcher.append(preferenceRow(downloaded?.path?'Ready to install':update?.state==='available'?`Version ${update.latest} available`:'Launcher updates',downloaded?.path?`Downloaded and verified · ${mb(downloaded.bytes)} MB. Restart the launcher to install.`:update?.note||'Check for a new launcher version. Your games and saves are preserved.',actions));
  // What changed lives on the site, not in a paragraph here.
  actions.append(el('button',{type:'button',text:'Patch notes',onclick:()=>call('open_website',{url:PATCH_NOTES_URL}).catch(()=>{})}));
  // The same row shape as "Launcher updates" above it, so it belongs to the
  // page instead of floating under a note (Adam, 2026-09-11).
  // Two presses, on purpose: one press on a pad, or a stray Enter, must not
  // remove the launcher. The first arms the button for a few seconds and
  // says so; the second does it.
  const uninstall=el('button',{type:'button',text:'Uninstall…',disabled:busy,onclick:()=>{
    if(uninstall.dataset.armed){act('Uninstall','uninstall_launcher',{confirm:true},()=>{});return;}
    uninstall.dataset.armed='true';uninstall.textContent='Press again to remove';
    setTimeout(()=>{if(!uninstall.isConnected)return;delete uninstall.dataset.armed;uninstall.textContent='Uninstall…';},8000);
  }});
  launcher.append(preferenceRow('Remove NFL Street Reborn','Removes the launcher and the game from this PC. Your saves stay.',uninstall));
  wrap.append(preferences,launcher,contentBlock());
  return wrap;
}

const GRAPHICS_PRESETS=[['performance','Performance','ReShade disabled.'],['balanced','Balanced','ReShade enabled with lighter effects.'],['quality','All-Out','ReShade enabled with high effects.']];
function graphicsBlock(){
  const wrap=dolphinBlock();
  // The presets choose ReShade's effects, and ReShade exists only on Windows.
  if(state.dolphin?.os&&state.dolphin.os!=='windows')return wrap;
  const group=el('section',{class:'preference-group'},el('h2',{text:'Visual effects'}));
  if(!ok(state.graphics))group.append(el('p',{class:'partial',text:state.graphics?.error||'Graphics settings are unavailable. Refresh settings to try again.'}));
  else{
    const saved=get('preset')||state.progress?.summary?.graphics||'balanced';
    const current=state.graphics.current;
    const selected=current?(current.label==='Previous settings'?'previous':'custom'):(saved==='ultra'?'quality':GRAPHICS_PRESETS.some(x=>x[0]===saved)?saved:'balanced');
    const select=el('select',{id:'graphics-preset',disabled:busy||state.dolphin?.editable===false});
    for(const [value,label] of GRAPHICS_PRESETS)select.append(el('option',{value,text:label}));
    if(state.graphics.previous||selected==='previous')select.append(el('option',{value:'previous',text:'Previous settings'}));
    if(selected==='custom')select.append(el('option',{value:'custom',text:'Custom',disabled:true}));
    select.value=selected;
    select.addEventListener('change',()=>act('Graphics preset','setup_save_graphics_options',{mode:select.value==='previous'?'previous':'preset',resolution:3,msaa:1,ssaa:false},()=>{if(select.value!=='previous')set('preset',select.value);}));
    const description=GRAPHICS_PRESETS.find(x=>x[0]===selected)?.[2]||(selected==='previous'?'Use the graphics settings imported from your previous installation.':'Your saved custom settings are selected.');
    group.append(preferenceRow('Graphics preset',description,select,'graphics-preset'));
    if(ok(state.hardware)&&state.hardware.description){
      const recommended=GRAPHICS_PRESETS.find(x=>x[0]===state.hardware.recommended)?.[1];
      group.append(preferenceRow('Your PC',state.hardware.description,recommended?el('span',{class:'settings-badge',text:`${recommended} recommended`}):el('span',{class:'note',text:'Choose your preferred effects'})));
    }
  }
  wrap.insertBefore(group,wrap.querySelector('.preference-group'));
  return wrap;
}


const preferenceGroups={
 General:[['confirm_stop','Confirm before stopping','Ask before closing a game.','bool'],['pause_unfocused','Pause when unfocused','Pause local play when you switch to another app.','bool'],['osd_messages','Game notifications','Show Dolphin messages over the game.','bool']],
 Graphics:[['internal_resolution','Internal resolution','How sharp the game renders. 3× is the Reborn default; lower it if the game runs slowly.','resolution'],['msaa','Anti-aliasing','Smooths edges. Off is the Reborn default and is what the game has always run at; raising it costs frame rate and softens the depth-based ReShade effects.','msaa'],['video_backend','Graphics backend','Direct3D 11 is the Reborn default. Vulkan or OpenGL can run better on some drivers, but the ReShade visual effects hook Direct3D, so they do not load on the other backends.','video_backend'],['sync_gpu','Synchronize GPU thread','Off is faster. Turn it on only if you see graphical glitches.','bool'],['vsync','V-Sync','Reduce screen tearing. May add a small amount of input delay.','bool'],['show_fps','Frame rate','Show a small frames-per-second counter.','bool']],
 Display:[['fullscreen','Start in fullscreen','Open the game in fullscreen mode.','bool'],['show_ping','Netplay ping','Show connection latency during online play.','bool'],['always_on_top','Keep game on top','Keep the game window above other windows.','bool']],
 Audio:[['audio_backend','Audio output','Default shares audio with other apps. Windows exclusive gives Dolphin sole use of the output device.','backend'],['volume','Game volume','Adjust Dolphin audio output.','range',0,100],['muted','Mute audio','Silence game audio without changing the volume.','bool'],['audio_stretch','Audio stretching','Smooth out audio during brief slowdowns.','bool'],['audio_latency','Audio buffer','Audio buffering in milliseconds. Raise it if sound crackles.','range',0,200]],
 Saves:[['slot_a','Memory Slot A','Choose where Player 1 saves game progress.','slot'],['card_a','Slot A memory card','Used when Slot A is set to Memory card. Leave blank for Dolphin’s default.','card'],['gci_a','Slot A save folder','Used when Slot A is set to GCI folder. Leave blank for the default folder.','folder'],['slot_b','Memory Slot B','An optional second card. Existing saves are kept when you change the type.','slot'],['card_b','Slot B memory card','Used when Slot B is set to Memory card.','card'],['gci_b','Slot B save folder','Used when Slot B is set to GCI folder.','folder']]
};
// The choices come from the launcher, per platform: Direct3D and WASAPI exist
// only on Windows, and a Mac plays on Metal. The lists below are the Windows
// answer, kept for a launcher that does not send its own.
const backendNames={D3D:'Direct3D 11',D3D12:'Direct3D 12',Vulkan:'Vulkan',OGL:'OpenGL',Metal:'Metal'};
const audioNames={Cubeb:'Default (shared audio)','WASAPI (Exclusive Mode)':'Windows exclusive'};
function choicesFor(key,fallback){const list=state.dolphin?.choices?.[key];return Array.isArray(list)&&list.length?list:fallback;}
// ReShade and Windows exclusive audio exist only on Windows, so the Windows
// wording would promise something another platform does not have.
function platformNote(key,description){
 const os=state.dolphin?.os;if(!os||os==='windows')return description;
 if(key==='video_backend')return os==='macos'?'Metal is the only graphics backend that loads the Reborn HD textures on a Mac.':'Vulkan is the Reborn default. OpenGL can run better on some drivers.';
 if(key==='msaa')return 'Smooths edges. Off is the Reborn default and is what the game has always run at; raising it costs frame rate.';
 if(key==='audio_backend')return 'Dolphin shares audio with other apps.';
 return description;
}
function dolphinBlock(){
 const group=el('section',{class:'preference-group'},el('h2',{text:settingsTab==='Saves'?'Memory cards & save data':settingsTab==='Graphics'?'Rendering':settingsTab}));
 if(!ok(state.dolphin)){group.append(el('p',{class:'partial',text:state.dolphin?.error||'Dolphin settings could not be read.'}));return el('div',{class:'settings-content'},group);}
 const canEdit=key=>state.dolphin.editable!==false||(state.dolphin.live_editable||[]).includes(key);
 const values=state.dolphin.values||{},status=el('p',{class:'settings-status',role:'status',text:settingsTab==='Audio'&&(state.dolphin.live_editable||[]).length?'Volume and mute apply immediately, including during NetPlay. Other audio settings apply next time.':state.dolphin.editable===false?(state.dolphin.readonly_reason||'Close Dolphin to edit these settings, then refresh.'):'Changes save automatically and take effect next time you open Dolphin.'});
 for(let [key,label,description,type,min,max] of preferenceGroups[settingsTab]||[]){
  description=platformNote(key,description);
  if(key==='audio_latency'&&!['WASAPI (Exclusive Mode)','OpenAL'].includes(values.audio_backend))continue;
  if(values[key]===undefined){group.append(el('p',{class:'partial',text:label+' is unavailable. Refresh settings to try again.'}));continue;}
  const id='preference-'+key,area=el('div',{class:'preference-value'});let control;
  if(type==='backend'){control=el('select',{id});const audio=choicesFor('audio_backend',['Cubeb','WASAPI (Exclusive Mode)']);for(const value of audio)control.append(el('option',{value,text:audioNames[value]||value}));if(!audio.includes(values[key]))control.append(el('option',{value:values[key],text:values[key]}));control.value=values[key];}
  else if(type==='slot'){control=el('select',{id});for(const [value,text] of [['8','GCI folder'],['1','Memory card'],['255','Nothing']])control.append(el('option',{value,text}));if(!['8','1','255'].includes(values[key]))control.append(el('option',{value:values[key],text:'Current device ('+values[key]+')'}));control.value=values[key];}
  else if(type==='resolution'){control=el('select',{id});for(const [value,text] of [['1','1× native (640×528)'],['2','2×'],['3','3× (Reborn default)'],['4','4×'],['5','5×'],['6','6×']])control.append(el('option',{value,text}));const n=String(parseInt(values[key])||3);if(!['1','2','3','4','5','6'].includes(n))control.append(el('option',{value:n,text:n+'×'}));control.value=n;}
  else if(type==='video_backend'){control=el('select',{id});const video=choicesFor('video_backend',['D3D','D3D12','Vulkan','OGL']);video.forEach((value,i)=>control.append(el('option',{value,text:(backendNames[value]||value)+(i===0?' (Reborn default)':'')})));if(!video.includes(values[key]))control.append(el('option',{value:values[key],text:values[key]}));control.value=values[key];}
  else if(type==='msaa'){control=el('select',{id});for(const [value,text] of [['1','Off (Reborn default)'],['2','2×'],['4','4×'],['8','8×']])control.append(el('option',{value,text}));const n=String(parseInt(values[key])||1);if(!['1','2','4','8'].includes(n))control.append(el('option',{value:n,text:n+'×'}));control.value=n;}
  else if(type==='bool')control=el('input',{id,type:'checkbox',checked:String(values[key]).toLowerCase()==='true'});
  else if(type==='range')control=el('input',{id,type:'range',min,max,step:1,value:values[key]});
  else control=el('input',{id,type:'text',value:values[key]||'',placeholder:'Use default'});
  control.dataset.preferenceKey=key;control.disabled=busy||!canEdit(key);area.append(control);
  const output=type==='range'?el('output',{for:id,text:values[key]+(key==='volume'?'%':' ms')}):null;if(output){area.append(output);control.addEventListener('input',()=>output.textContent=control.value+(key==='volume'?'%':' ms'));}
  const save=async()=>{if(busy)return;const hadFocus=document.activeElement===control;document.getElementById("screen").setAttribute("aria-busy","true");const previous=values[key],value=type==='bool'?String(control.checked):control.value;busy=true;group.querySelectorAll('input,select,button').forEach(n=>n.disabled=true);status.textContent='Saving…';try{await call('dolphin_preference_set',{runtime:get('runtime'),key,value});values[key]=value;status.textContent=label+' saved.';}catch(e){if(type==='bool')control.checked=String(previous).toLowerCase()==='true';else control.value=previous||'';if(output)output.textContent=control.value+(key==='volume'?'%':' ms');status.textContent=reason(e);}finally{busy=false;document.getElementById('screen').removeAttribute('aria-busy');group.querySelectorAll('input,select,button').forEach(n=>n.disabled=!canEdit(n.dataset.preferenceKey));if(hadFocus)control.focus({preventScroll:true});if(key==='audio_backend'){draw();document.getElementById(id)?.focus({preventScroll:true});}}};
  control.addEventListener('change',save);
  if(type==='card'||type==='folder'){area.append(el('button',{type:'button',text:'Browse','data-preference-key':key,'aria-label':'Browse '+label,disabled:control.disabled,onclick:async()=>{try{const picked=await call('pick_path',{kind:type==='card'?'memory_card':'folder'});if(picked){control.value=picked;await save();}}catch(e){status.textContent=reason(e);}}}));}
  group.append(el('div',{class:'preference-row'},el('div',{class:'preference-description'},el('label',{for:id,text:label}),el('p',{text:description})),area));
 }
 const wrap=el('div',{class:'settings-content'},status,group);
 if(settingsTab==='Saves')wrap.append(el('button',{type:'button',text:'Open save folder',onclick:async()=>{try{await call('dolphin_saves_folder',{runtime:get('runtime')});}catch(e){status.textContent=reason(e);}}}));
 
 return wrap;
}
function draw() {
 const screen=document.getElementById('screen'),parts=[];
 if(problem)parts.push(refusal(problem));
 const status=el('p',{class:'settings-status',role:'status'});
 parts.push(el('div',{class:'settings-toolbar'},el('div',{},el('h2',{text:'NFL Street Reborn Dolphin'}),el('p',{text:"Everyday settings below. Open Dolphin for its complete set of tools. Reborn's textures apply to any NFL Street 2 disc you open here, so another copy will show Reborn's players."})),el('button',{type:'button',text:'Open Dolphin',onclick:async()=>{try{await call('open_runtime',{runtime:get('runtime'),userDir:get('userDir')});status.textContent='Dolphin opened.';await refresh();}catch(e){status.textContent=reason(e);}}})),status);
 const tabs=el('div',{class:'section-tabs','aria-label':'Settings sections'});for(const name of ['General','Graphics','Display','Audio','Saves','Updates','Files & storage'])tabs.append(el('button',{type:'button',text:name,'aria-pressed':String(name===settingsTab),disabled:busy,onclick:()=>{if(busy)return;settingsTab=name;draw();}}));parts.push(tabs);
 if(settingsTab==='Updates')parts.push(updatesBlock());else if(settingsTab==='Graphics')parts.push(graphicsBlock());else if(settingsTab==='Files & storage')parts.push(repairBlock(),unverifiedChoice(),boxesBlock(),foundBlock(),whereBlock());else parts.push(dolphinBlock());
 parts.push(el('button',{type:'button',text:'Refresh settings',disabled:busy,onclick:refresh}));
 // `repairBlock` and `unverifiedChoice` draw nothing most of the time, and a
 // null handed to replaceChildren becomes the word "null" on the page.
 screen.replaceChildren(...parts.filter(Boolean));
 // A real browser hides a <dialog> until it is opened; the page renders the
 // same card either way, so a runtime without showModal still shows the choice.
 if(unverified)document.getElementById('unverified-game')?.showModal?.();
}

async function refresh() {
  problem = null;
  try {
    await load();
  } catch (e) {
    problem = reason(e);
  }
  draw();
}

// Dolphin can close while this page stays open, including when it was opened
// outside the launcher. Refresh availability without rebuilding controls on
// every poll or overwriting a volume slider the player is using.
let pollingDolphin=false;
async function refreshDolphinAvailability(){
  if(pollingDolphin||busy||document.hidden||['Updates','Files & storage'].includes(settingsTab)||!state.dolphin)return;
  pollingDolphin=true;
  try{
    const next=await call('dolphin_preferences',{runtime:get('runtime')});
    if(busy||!next||['Updates','Files & storage'].includes(settingsTab))return;
    const availability=value=>JSON.stringify([value.editable,value.live_editable,value.readonly_reason]);
    const changed=availability(next)!==availability(state.dolphin);
    state.dolphin=next;
    if(changed){const focused=document.activeElement?.id;draw();if(focused)document.getElementById(focused)?.focus({preventScroll:true});}
  }catch(_){/* A failed read is not evidence that Dolphin stopped. */}
  finally{pollingDolphin=false;}
}
document.addEventListener('session-state',refreshDolphinAvailability);
window.addEventListener('focus',refreshDolphinAvailability);

// A content check or download can outlive the page that started it.
// Update only its group so the preference being edited keeps its focus.
let pollingContent=false;
setInterval(async()=>{
  if(pollingContent||busy||settingsTab!=='Updates'||document.hidden||!['checking','fetching','installing'].includes(state.content?.phase))return;
  pollingContent=true;
  try{
    const next=await call('content_status');
    if(busy||settingsTab!=='Updates')return;
    if(JSON.stringify(next)!==JSON.stringify(state.content)){
      state.content=next;
      document.getElementById('settings-game-content')?.replaceWith(contentBlock());
    }
  }catch(e){problem=reason(e);}
  finally{pollingContent=false;}
},2000);

const pill = document.getElementById("bridge");
if (pill) {
  pill.textContent = bridge ? "Launcher connected" : "Preview";
  pill.className = "pill " + (bridge ? "ok" : "bad");
}
markCurrent();
wireCoreFold();
if (bridge) {
  refresh();
} else {
  problem = "The launcher core is not attached. Open this page inside the launcher window.";
  draw();
}
