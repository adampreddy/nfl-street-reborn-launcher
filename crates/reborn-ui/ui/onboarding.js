// First-run screens. The native journal owns completion and stage order.
// Review can reopen reversible choices, then returns directly to the recap.

const bridge = window.__TAURI__ ? window.__TAURI__.core : null;
// One size while first run is incomplete: every setup screen is laid out
// for it, and a resized window put a scrollbar on the controller step.
if (bridge) bridge.invoke("setup_window_lock", { locked: true }).catch(() => {});

// A harness fakes document without querySelectorAll; the nav is not its concern.
for (const a of typeof document.querySelectorAll === "function" ? document.querySelectorAll(".shell-nav a") : []) {
  if ((a.getAttribute("href") || "").toLowerCase() === "onboarding.html") a.classList.add("current");
  // The keyboard reaches a link that pointer-events cannot stop.
  if (a.classList.contains("setup-exit")) a.addEventListener("click", (e) => { if (moving) e.preventDefault(); });
}

function legacyRoot() {
  return null; // The backend resolves the saved installation for every setup command.
}

// Filled by refresh(), which is the only thing that talks to onboarding_progress.
let state = null;
// Per screen scratch, cleared on every move so an answer cannot leak forward.
let pending = {};
let installation = null;
let installInfo = null;
let moving = false;
let reviewReturn = false;

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
  const answer = await bridge.invoke(cmd, { legacyRoot: legacyRoot(), ...args });
  log(cmd, answer === undefined ? "ok" : answer);
  return answer;
}

/// Ask where the flow stands, then draw it. Every move ends here.
async function refresh(reviewStage = null) {
  try {
    state = await call("onboarding_progress");
    if (typeof reviewStage === 'string' && !state.complete) {
      const step = state.plan?.find(step => step.stage === reviewStage && (!step.irreversible || reviewStage === 'preparation'));
      if (step) state.next = {...state.next, kind:'show', stage:step.stage, position:step.position, total:step.total};
    }

    // Showing a stage IS reaching it, and the journal has to say so. Two
    // stages refuse their own work without the marker, preparation and
    // completion, and without this the flow stopped dead on both with a
    // refusal written for a programmer: "call reach() first".
    //
    // Safe to do on every draw. reach is idempotent and deliberately
    // permissive, so re-showing a screen records nothing new, and the marker
    // changes nothing about what is drawn.
    if (state.next.kind === "show" || state.next.kind === "reconcile") {
      await call("onboarding_reach", { stage: state.next.stage });
    }

    try {
      installation = await call("launch_check", {runtime:null, game:null, userDir:null});
    } catch (e) { installation = {ready:false, problem: String(e.message || e)}; }
    try { installInfo = await call("setup_install_info"); } catch (_) { installInfo = null; }
    pending.runtime = installation?.runtime || null;
    await prefetch(state.next.stage);
    draw();
  } catch (e) {
    fail(e);
  }
}

/// Fetch what a screen needs before drawing it.
///
/// Only the screens that read something outside the journal. The controller
/// screen is deliberately NOT here: a pad list goes stale the moment a wireless
/// pad sleeps, so that one is fetched on a button the player can press again
/// rather than once on the way in.
async function scanPads() {
  // Indices belong to one scan. Never reuse a selection after devices change.
  delete pending.port;
  try {
    const report = await call("devices", { runtime: pending.runtime || null });
    pending.pads = {list: report.pads || [], searched_everywhere: report.complete, unsearched: report.unsearched || [], source: report.source};
  } catch (e) { pending.pads = {error: String(e.message || e)}; }
}

async function prefetch(slug) {
  const summary=state.summary||{};
  if(slug==='system_check'||slug==='preparation') {
    try{pending.graphicsOptions=await call('setup_graphics_options');}catch(_){pending.graphicsOptions={};}
  }
  if(slug==='system_check') {
    pending.graphicsMode ??= pending.graphicsOptions?.current?.label==='Previous settings'?'previous':pending.graphicsOptions?.current?'custom':'preset';
    pending.resolution ??= pending.graphicsOptions?.current?.resolution||3;
    pending.aa ??= (pending.graphicsOptions?.current?.ssaa?'ssaa:':'msaa:')+(pending.graphicsOptions?.current?.msaa||1);
    if(!pending.hardware) {try{pending.hardware=await call('setup_hardware_info');}catch(_){pending.hardware={};}}
    pending.preset ??= summary.graphics || pending.hardware?.recommended || undefined;
  }
  if(slug==='controls_profile'){let last;try{last=sessionStorage.getItem('setup:texture-controller');sessionStorage.setItem('setup:texture-controller',summary.controller||'');}catch(_){}if(last===(summary.controller||''))pending.profile ??= summary.controls_profile||undefined;}
  if(slug==='updates' && typeof summary.automatic_updates==='boolean') pending.automatic ??= summary.automatic_updates?'yes':'no';
  if (slug === "controller_setup" && !pending.pads) { await scanPads(); if(pending.pads?.list?.length===1) pending.port="0"; if(summary.controller){const i=pending.pads?.list?.findIndex(p=>p.device_string===summary.controller);if(i>=0)pending.port=String(i);}else if(summary.no_controller_because)pending.port="none"; }
  // Migration boots the game the installer carries when there is one, never the old disc.
  // A game already imported (the booted copy lives under library\offline, the
  // pristine under library\imported) survives a restart of the wizard.
  if(slug==='game_verification'&&!pending.image)pending.image=installInfo?.mode==='fresh'?(/[\\/]library[\\/](imported|offline)[\\/]/.test(installation?.game||'')?installation.game:null):(installInfo?.shipped_game||installInfo?.games?.[0]||null);
  if(slug==='game_verification')pending.installRoot ??= installInfo?.root;
  if (slug === "controls_profile" && !pending.controls) {
    try {
      // The suggestion comes from the pad that was actually found, so this
      // needs the runtime path for the search to cover every backend.
      const list = await call("controls_options", {
        runtime: pending.runtime || null,
      });
      pending.controls = { list };
    } catch (e) {
      pending.controls = { error: typeof e === "string" ? e : e.message };
    }
  }
}

function showStepError(error) {
  const host=document.getElementById('screen');
  host.querySelector('.step-error')?.remove();
  const message=String(error?.message||error);
  const running=/close.*dolphin|dolphin.*running/i.test(message);
  const notice=el('div','step-error');notice.setAttribute('role','alert');
  notice.append(el('strong',null,running?'Close Reborn Dolphin to continue':'This step couldn’t be completed'),el('p',null,running?'Your settings are safe. Close the emulator, then try again.':message));
  const summary=host.querySelector('.detected-install');
  if(summary)host.insertBefore(notice,summary);else host.append(notice);
  notice.scrollIntoView?.({block:'nearest',behavior:'instant'});
}

function releaseLabel(release) {
  if(!release)return 'Version not recorded';
  if(release.release)return `Release ${release.release}`;
  const format=value=>value?.replace(/^week[ _-]?(\d+)$/i,'Week $1');
  if(release.game&&release.game===release.textures)return format(release.game);
  return [release.game?`Game: ${format(release.game)}`:null,release.textures?`Textures: ${format(release.textures)}`:null].filter(Boolean).join(' · ');
}

function fail(e) {
  current?.dispose?.();
  current = null;
  const msg = typeof e === "string" ? e : e.message || JSON.stringify(e);
  log("error", msg);
  document.getElementById("screen").replaceChildren(
    el("h1", null, "Something went wrong"),
    // The core's own sentence. Rewriting it here would lose the part that says
    // what to do about it.
    el("div", "refusal", msg),
    (() => { const retry = el("button", "primary", "Try again"); retry.addEventListener("click", refresh); return retry; })(),
  );
  updateNav();
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

// One-time notices the launcher owes a returning player, drawn on the
// controller step. The backend decides what is pending and what it says
// (notices_pending / notice_dismiss, the same two commands Controllers
// uses); this only draws it and reports "Got it". `onShown` gets true when
// a notice is on screen and false again once the last one is dismissed.
async function mountSetupNotices(host, onShown) {
  if (!bridge) return;
  let list = [];
  try { list = await bridge.invoke("notices_pending"); } catch (_) { return; }
  if (!Array.isArray(list) || !list.length) return;
  for (const notice of list) {
    if (!notice || typeof notice.id !== "string") continue;
    const box = el("section", "notice migration");
    box.setAttribute("role", "status");
    box.dataset.notice = notice.id;
    const got = el("button", "notice-dismiss", "Got it");
    got.type = "button";
    got.onclick = async () => {
      try { await bridge.invoke("notice_dismiss", { id: notice.id }); } catch (_) {}
      box.remove();
      if (!host.childElementCount && onShown) onShown(false);
    };
    box.append(el("strong", null, notice.title || ""), el("p", null, notice.text || ""), got);
    host.append(box);
  }
  if (host.childElementCount && onShown) onShown(true);
}

function el(tag, cls, text) {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text !== undefined && text !== null) node.textContent = text;
  return node;
}

/// The step strip, from the plan the core returns rather than a list here.
function drawSteps() {
  const host = document.getElementById("steps");
  host.replaceChildren();
  const plan = state.plan || [];
  for (const step of plan) {
    const dot = el("span", step.stage === state.next.stage ? "now" : step.satisfied ? "done" : "");
    dot.title = titleFor(step.stage);
    dot.setAttribute("aria-label", `${titleFor(step.stage)}${step.satisfied ? ": complete" : ""}`);
    if (step.stage === state.next.stage) dot.setAttribute("aria-current", "step");
    host.appendChild(dot);
  }
}

function heading(title, next) {
  const out = [el("h1", null, title)];
  if (next.position && next.total) {
    out.push(el("p", "position", `Step ${next.position} of ${next.total}`));
  }
  return out;
}

/// Facts as a table, with absent values drawn as absent.
function factsTable(rows) {
  const table = el("table", "facts");
  for (const [label, value] of rows) {
    const tr = el("tr");
    tr.appendChild(el("th", null, label));
    const td = el("td");
    if (value === null || value === undefined) {
      td.appendChild(el("span", "absent", "not measured"));
    } else {
      td.textContent = value;
    }
    tr.appendChild(td);
    table.appendChild(tr);
  }
  return table;
}

function radios(name, options) {
  const out = [];
  for (const opt of options) {
    const label = el("label", "choice");
    const input = document.createElement("input");
    input.type = "radio";
    input.name = name;
    input.value = opt.value;
    if (pending[name] === opt.value) input.checked = true;
    input.addEventListener("change", () => {
      pending[name] = opt.value;
      updateNav();
    });
    label.appendChild(input);
    label.appendChild(el("span", "what", opt.what));
    if (opt.why) label.appendChild(el("span", "why", opt.why));
    out.push(label);
  }
  return out;
}

// ---------------------------------------------------------------------------
// The screens
//
// One entry per stage slug. Each returns the nodes to draw, and says whether
// Next can fire and what it does. `answer` is what Next calls, and a stage with
// no answer is one that only needs acknowledging.
// ---------------------------------------------------------------------------

function installationPanel() {
  const panel = el("section", "installation-check");
  panel.appendChild(el("h2", null, installation?.ready ? "Game found" : "Locate your installation"));
  const details = el("details", "setup-details");
  details.appendChild(el("summary", null, installation?.ready ? "Installation details" : "Choose files separately"));
  if (installation?.ready) {
    panel.appendChild(el("p", "note", "Your game and Dolphin are ready to use."));
    details.appendChild(factsTable([["Game", installation.game], ["Dolphin", installation.runtime]]));
  } else {
    panel.appendChild(el("p", "note", "Choose the folder where you installed Reborn."));
    if (installation?.problem) details.appendChild(el("p", "note", installation.problem));
  }
  const actions = el("div", "settings-actions");
  for (const [label, kind, which] of [["Choose installation folder", "folder", "legacy_root"], ["Choose game image", "game", "game"], ["Choose Dolphin", "runtime", "runtime"]]) {
    const button = el("button", null, label);
    button.type = "button";
    button.addEventListener("click", async () => {
      if (moving) return;
      moving = true; updateNav(); button.disabled = true;
      try {
        const value = await call("pick_path", {kind});
        if (value) {
          if(which === "legacy_root") await call("setup_use_existing", {root:value});
          else {
            const stored = await call("settings_set_path", {which, value});
            // Nothing was stored, because the file is NFL Street 2 but not a
            // Reborn build. The wizard has its own repair one screen away, so
            // this says what is wrong rather than offering the escape hatch
            // here: the acknowledgment belongs to the Settings page, behind a
            // choice the player is actually shown.
            if (stored && stored.unverified) {
              panel.appendChild(el("div", "refusal", stored.unverified.headline + " " + stored.unverified.detail));
              return;
            }
          }
          try { localStorage.setItem("h:" + (which === "legacy_root" ? "legacyRoot" : which), value); } catch (_) {}
          pending = {};
        }
        await refresh();
      } catch (e) { panel.appendChild(el("div", "refusal", String(e.message || e))); }
      finally { moving = false; button.disabled = false; updateNav(); }
    });
    if (!installation?.ready && which === "legacy_root") {
      button.className = "primary"; panel.appendChild(button);
    } else actions.appendChild(button);
  }
  details.appendChild(actions); panel.appendChild(details);
  return panel;
}

export const outcomeWords=(outcome,label,note,patched)=>{
  const build=label||'NSRV3';
  switch(outcome){
    case 'bundle':return ['Game File Patched!',`Your original NFL Street 2 disc was patched to ${build} and verified. Continue to your controller.`];
    case 'patched':return ['Game File Patched!',`Your disc was updated to ${build} and verified. Continue to your controller.`];
    case 'newest':return ['Game File Imported!',`This disc is already the newest Reborn build (${build}). Continue to your controller.`];
    case 'unconverted':return ["Your game file isn't NFL Street Reborn",`This is NFL Street 2, but not a Reborn build, so rosters, faces and stats won't match. This usually means setup couldn't convert your disc.${note?' '+note:''}`];
    case 'different-game':return ['This is not NFL Street 2',`${note?note+' ':''}Reborn's roster, textures and updates do not apply to it.`];
    case 'unreadable':return ['Your game file could not be read',note||'Choose an NFL Street 2 (USA) disc image.'];
    case 'unknown':case 'no-chain':return ["Your game file isn't NFL Street Reborn",'Reborn could not confirm this disc is one of its builds, so it was left exactly as it is. Repair it before playing.'];
    // Verified as a Reborn build, but the update step could not run. That is a
    // patch failure, not a verdict on the disc, and saying "isn't NFL Street
    // Reborn" about a disc Reborn just recognized is how three surfaces ended
    // up disagreeing about one file.
    case 'not-updated':return ['Your game is ready',`This is an NFL Street Reborn build. The update to the newest version could not be applied right now, and the launcher will try again after setup.${note?' '+note:''}`];
    // Recovery: the complete game came from Reborn itself, so there is no
    // disc of the player's to report on. The note is what setup did and why,
    // present when recovery ran because a chosen disc would not convert, and
    // absent when the player asked for the download outright. It has to be
    // shown either way: it is the only place the player is told their own
    // file was left alone.
    case 'downloaded':return ['NFL Street Reborn is ready',note?note+' Continue to your controller.':'The complete Reborn game was downloaded and verified. Continue to your controller.'];
    default:return patched?['Game File Patched!','Patched and verified. Continue to your controller.']:['Game File Imported!','Imported and ready. Continue to your controller.'];
  }
};

const SCREENS = {
  welcome: () => ({
    nodes: [
      el("p", "setup-intro", "Let's get you ready to play."),
      ...radios("install", [
        {value:"fresh",what:"Start fresh",why:"Set up Reborn using your own copy of NFL Street 2."},
        {value:"migrate",what:"I already have Reborn",why:"Find your game and bring your saves with you."},
      ]),
    ],
    ready: () => !!pending.install,
    nextLabel: "Continue",
    answer: async () => { await call("setup_install_choice", {mode:pending.install}); return call("onboarding_acknowledge"); },
  }),

  migration_offer: () => ({
    nodes: [
      el(
        "p",
        null,
        "We found an earlier Reborn installation.",
      ),
      el(
        "p",
        null,
        "Keep your progress. Your existing saves stay safe.",
      ),
      ...(state.migration_blocked
        ? [el("div", "refusal", state.migration_blocked)]
        : []),
      ...radios("migration", [
        { value: "yes", what: "Keep my saves", why: "Bring your memory cards into Reborn. Existing saves are never replaced." },
        { value: "no", what: "Skip import", why: "The old install is left alone" },
      ]),
    ],
    // Declining is always available, even when the copy is blocked: leaving
    // the old install alone is exactly what a blocked migration should let you
    // do, and refusing to let anyone past would strand them here.
    ready: () => pending.migration === "no" || (pending.migration === "yes" && !state.migration_blocked),
    answer: () => {
      if (pending.migration === "no") return call("onboarding_decline_migration");
      // Records the decision only. The copy is a separate, confirmed step,
      // because it moves memory cards and a Next button is not consent for
      // that.
      return call("onboarding_accept_migration", { legacyRoot: legacyRoot() });
    },
  }),

  migration_copy: () => ({
    nodes: [
      el(
        "p",
        null,
        "Your memory cards and screenshots will be copied. Your mods stay where they are, and existing saves are never overwritten.",
      ),
      el(
        "div",
        "partial",
        "Keep the launcher open while it copies. If interrupted, setup will offer to resume.",
      ),
    ],
    ready: () => true,
    nextLabel: "Copy it over",
    answer: async () => {
      const done = await call("migration_apply", {
        savesOnly: true,
        legacyRoot: legacyRoot(),
        confirm: true,
      });
      if (done.journal_note) {
        // The files moved but the journal would not take it. Saying nothing
        // would leave the flow looking stuck for an invisible reason.
        document
          .getElementById("screen")
          .appendChild(el("div", "partial", done.journal_note));
      }
      return done;
    },
  }),

  game_verification: () => {
    if (installInfo?.mode === "fresh") {
      const box=el('section','patch-card');
      const isPatched=()=>{try{return localStorage.getItem('setup:patched:'+pending.image)==='true';}catch(_){return false;}};
      const isShipped=()=>{try{return localStorage.getItem('setup:shipped:'+pending.image)==='true';}catch(_){return false;}};
      const storedWords=()=>{try{return outcomeWords(localStorage.getItem('setup:outcome:'+pending.image)||'',localStorage.getItem('setup:label:'+pending.image)||'',localStorage.getItem('setup:note:'+pending.image)||'',isPatched());}catch(_){return outcomeWords('', '', '', false);}};
      // The two endings a player cannot go on from: a disc Reborn could not
      // convert, and one it could not read. Nothing else, and in particular
      // never a disc that converted.
      // A disc Reborn could not end up vouching for. 'unknown' and 'no-chain'
      // join the list now that a genuine Reborn build whose patch step merely
      // refused is reported as 'not-updated' instead: what is left on these
      // two is a disc that is not a Reborn build, which is exactly what the
      // recovery download exists for.
      const blocked=()=>{try{return ['unconverted','unreadable','unknown','no-chain'].includes(localStorage.getItem('setup:outcome:'+pending.image)||'');}catch(_){return false;}};
      const shipped=installInfo?.shipped_game||null;
      const status=el('p','import-status',pending.image?(isShipped()?'NFL Street Reborn is ready':storedWords()[0]):shipped?'Preparing NFL Street Reborn…':'Your original file stays untouched.');status.setAttribute('role','status');
      const progress=el('progress','patch-progress');progress.max=100;progress.hidden=!pending.image&&!shipped;if(pending.image)progress.value=100;
      const detail=el('p','note',pending.image?(isShipped()?'Included with this install. Continue to your controller.':storedWords()[1]):shipped?'The game is included with this install. Keep the launcher open.':'ISO · RVZ · CISO · NKit ISO');
      const browse=el('button',pending.image||shipped?'':'primary',pending.image||shipped?'Use a different game file':'Browse for NFL Street 2');
      // The game the installer carries is imported by itself: no disc asked
      // for. The button stays for a player who wants their own copy instead.
      if(shipped&&!pending.image&&!pending.shippedStarted){
        pending.shippedStarted=true;
        setTimeout(async()=>{
          if(moving)return;moving=true;browse.disabled=true;updateNav();progress.removeAttribute('value');
          try{
            const path=await call('setup_import_existing_game',{image:shipped});
            pending.image=path;try{localStorage.setItem('setup:shipped:'+path,'true');localStorage.setItem('h:game',path);}catch(_){}
            progress.value=100;box.classList.add('patch-complete');status.textContent='NFL Street Reborn is ready';detail.textContent='Included with this install. Continue to your controller.';
          }catch(e){status.textContent='Your game could not be prepared';detail.textContent=String(e.message||e);progress.hidden=true;pending.shippedStarted=false;}
          finally{moving=false;browse.disabled=false;updateNav();if(pending.image)document.getElementById('next').focus({preventScroll:true});}
        },0);
      }
      box.classList.toggle('patch-complete',!!pending.image);box.append(status,progress,detail,browse);
      // The way out for a player whose own copy cannot be converted: the
      // complete built game, downloaded. The button is built every draw
      // because the import that decides whether it is needed has not run
      // yet, so what decides whether anyone sees it is this one guard. It
      // joins the card only while the disc is one we could not convert or
      // could not read, and leaves again the moment that stops being true.
      const recover=el('button','primary','Download the Reborn game (1.36 GB)');recover.type='button';
      const syncRecovery=()=>{if(blocked()){if(recover.parentNode!==box)box.append(recover);}else recover.remove();};
      syncRecovery();
      recover.addEventListener('click',async()=>{
        if(moving)return;moving=true;recover.disabled=true;browse.disabled=true;updateNav();let unlisten;
        try{
          box.classList.remove('patch-complete');status.textContent='Downloading NFL Street Reborn…';detail.textContent='Keep the launcher open. This is a large download.';progress.hidden=false;progress.removeAttribute('value');
          if(window.__TAURI__?.event?.listen) unlisten=await window.__TAURI__.event.listen('game-import-progress',e=>{
            const data=e.payload;status.textContent=typeof data==='string'?data:data.stage;
            if(typeof data.percent==='number'){progress.value=data.percent;progress.setAttribute('aria-label',`Downloading the game: ${data.percent}%`);detail.textContent=`${data.percent}% complete`;}
            else {progress.removeAttribute('value');detail.textContent='Keep the launcher open. This is a large download.';}
          });
          const path=await call('setup_download_reborn_game');
          pending.image=path;
          try{localStorage.setItem('setup:patched:'+path,'false');localStorage.setItem('setup:outcome:'+path,'downloaded');localStorage.setItem('setup:label:'+path,'');localStorage.setItem('setup:note:'+path,'');localStorage.setItem('h:game',path);}catch(_){}
          const words=outcomeWords('downloaded','','',false);
          progress.value=100;box.classList.add('patch-complete');status.textContent=words[0];detail.textContent=words[1];browse.textContent='Use a different game file';browse.className='';
        }catch(e){status.textContent='The Reborn game could not be downloaded';detail.textContent=String(e.message||e);progress.hidden=true;}
        finally{unlisten?.();moving=false;recover.disabled=false;browse.disabled=false;updateNav();syncRecovery();if(pending.image&&!blocked())document.getElementById('next').focus({preventScroll:true});}
      });
      browse.addEventListener('click',async()=>{
        if(moving)return;moving=true;browse.disabled=true;updateNav();let unlisten;
        try {
          const image=await call('pick_path',{kind:'game'});if(!image)return;
          pending.image=null;box.classList.remove('patch-complete');status.textContent='Reading your disc…';progress.hidden=false;progress.removeAttribute('value');detail.textContent='Keep the launcher open while your game is prepared.';
          if(window.__TAURI__?.event?.listen) unlisten=await window.__TAURI__.event.listen('game-import-progress',e=>{
            const data=e.payload;status.textContent=typeof data==='string'?data:data.stage;
            if(typeof data.percent==='number'){progress.value=data.percent;progress.setAttribute('aria-label',`Preparing game: ${data.percent}%`);detail.textContent=`${data.percent}% complete`;}
            else {progress.removeAttribute('value');detail.textContent='Reading the game files. This can take a few minutes.';}
          });
          const imported=await call('setup_import_game',{image});
          pending.image=typeof imported==='string'?imported:imported.path;
          try{localStorage.setItem('setup:patched:'+pending.image,String(imported.patched===true));localStorage.setItem('setup:outcome:'+pending.image,String(imported.outcome||''));localStorage.setItem('setup:label:'+pending.image,String(imported.label||''));localStorage.setItem('setup:note:'+pending.image,String(imported.note||''));}catch(_){}
          try{localStorage.setItem('h:game',pending.image);}catch(_){}
          const words=outcomeWords(imported.outcome||'',imported.label||'',imported.note||'',imported.patched===true);
          progress.value=100;box.classList.add('patch-complete');status.textContent=words[0];detail.textContent=words[1];browse.textContent='Choose a different file';browse.className='';
        }catch(e){status.textContent='Your game could not be prepared';detail.textContent=String(e.message||e);progress.hidden=true;}
        finally{unlisten?.();moving=false;browse.disabled=false;updateNav();syncRecovery();if(pending.image&&!blocked())document.getElementById('next').focus({preventScroll:true});}
      });
      return {nodes:[el('p',null,shipped?'NFL Street Reborn is included with this install.':'Choose your legally obtained copy of NFL Street 2 (USA) for GameCube.'),box],ready:()=>!!pending.image&&!blocked(),nextLabel:'Continue',answer:()=>call('onboarding_confirm_game',{image:pending.image})};
    }
    const summary=el('section','detected-install');
    const header=el('div','detected-header');
    const icon=el('span','detected-icon',pending.installRoot?'✓':'⌕');icon.setAttribute('aria-hidden','true');
    const heading=el('div');heading.append(el('span','detected-eyebrow',pending.installRoot?'INSTALLATION FOUND':'FIND YOUR INSTALLATION'),el('h2',null,pending.installRoot?'NFL Street Reborn':'Locate your Reborn folder'));
    header.append(icon,heading,el('span','detected-release',releaseLabel(installInfo?.installed_release)));summary.append(header);
    const root=el('p','detected-path',pending.installRoot||'We couldn’t find an existing installation on this PC.');summary.append(root);
    const list=el('dl','detected-items');
    const row=(label,value,found)=>{const item=el('div','detected-item');item.append(el('dt',null,label));const detail=el('dd');const mark=el('span',found?'detected-check':'detected-missing',found?'✓':'—');mark.setAttribute('aria-hidden','true');detail.append(mark,el('span',null,value));item.append(detail);list.append(item);return detail.lastChild;};
    const gameName=path=>path===installInfo?.shipped_game?'NFL Street Reborn (included)':path.split(/[\\/]/).pop();
    const gameValue=row('Game file',pending.image?gameName(pending.image):'No game found',!!pending.image);
    const cards=installInfo?.memory_cards||[];
    row('Save data',cards.length?'Existing saves found':'No existing saves found',!!cards.length);
    row('Settings',installInfo?.settings_found?'Previous preferences found':'Use recommended defaults',!!installInfo?.settings_found);
    summary.append(list);
    const overrides=el('details','import-overrides');overrides.open=!pending.installRoot||!pending.image;
    overrides.append(el('summary',null,'Change detected files'));
    const fields=el('div','import-override-fields');
    const feedback=el('p','override-feedback');feedback.setAttribute('role','status');feedback.hidden=true;
    const runOverride=async(action)=>{if(moving)return;moving=true;updateNav();fields.setAttribute('aria-busy','true');for(const button of fields.querySelectorAll('button'))button.disabled=true;try{await action();}catch(e){feedback.textContent=String(e.message||e);feedback.hidden=false;}finally{moving=false;fields.removeAttribute('aria-busy');for(const button of fields.querySelectorAll('button'))button.disabled=false;updateNav();}};
    const browse=el('button',null,'Choose installation folder');browse.type='button';browse.onclick=()=>runOverride(async()=>{const folder=await call('pick_path',{kind:'folder'});if(folder){await call('setup_use_existing',{root:folder});pending={};await refresh('game_verification');}});
    const rescan=el('button',null,'Scan again');rescan.type='button';rescan.onclick=()=>runOverride(async()=>{pending={};await refresh('game_verification');});
    const locationActions=el('div','import-location-actions');locationActions.append(browse,rescan);fields.append(locationActions);
    const games=el('select');games.id='import-game';games.setAttribute('aria-label','Reborn game file');
    for(const path of new Set([...(installInfo?.shipped_game?[installInfo.shipped_game]:[]),...(installInfo?.games||[]),...(pending.image?[pending.image]:[])])){const option=el('option',null,gameName(path));option.value=path;games.append(option);}
    if(!games.options.length){const option=el('option',null,'Choose a game file');option.value='';games.append(option);}
    if(pending.image)games.value=pending.image;
    const updateGame=()=>{pending.image=games.value;gameValue.textContent=pending.image?gameName(pending.image):'No game found';gameValue.previousSibling.className=pending.image?'detected-check':'detected-missing';gameValue.previousSibling.textContent=pending.image?'✓':'—';updateNav();};
    games.onchange=updateGame;
    const choose=el('button',null,'Browse for a game file');choose.type='button';choose.onclick=()=>runOverride(async()=>{const image=await call('pick_path',{kind:'game'});if(image){if(!Array.from(games.options).some(o=>o.value===image)){const option=el('option',null,image.split(/[\\/]/).pop());option.value=image;games.append(option);}games.value=image;updateGame();}});
    const gameLabel=el('label',null,'Game file');gameLabel.htmlFor=games.id;
    const gameActions=el('div','import-game-actions');gameActions.append(games,choose);fields.append(gameLabel,gameActions);
    if(cards.length){const saves=el('div','detected-save-paths');saves.append(el('span',null,'Detected save locations'));for(const card of cards)saves.append(el('p',null,card));fields.append(saves);}
    fields.append(el('p','note','Choose another installation folder to change the source of your saves and settings.'),feedback);overrides.append(fields);
    const note=el('p','import-assurance','Controller, button textures and update preferences come next.');
    const loading=el('section','patch-card');loading.hidden=true;
    const status=el('p','import-status','Preparing import...');status.setAttribute('role','status');
    const progress=el('progress','patch-progress');progress.max=100;
    const detail=el('p','note','Keep the launcher open. Your original files stay safe.');loading.append(status,progress,detail);
    return {nodes:[el('p',null,pending.installRoot?'Your game, progress and preferences. All in one place.':'Choose your old folder. We’ll find your game, saves and settings.'),summary,overrides,note,loading],ready:()=>!!pending.installRoot&&!!pending.image,nextLabel:'Import and continue',answer:async()=>{
      if(reviewReturn){const image=await call('setup_import_existing_game',{image:pending.image});return call('onboarding_confirm_game',{image});}
      let unlisten;loading.hidden=false;for(const node of [summary,overrides,note])node.hidden=true;
      try{
        if(window.__TAURI__?.event?.listen)unlisten=await window.__TAURI__.event.listen('install-import-progress',e=>{const data=e.payload;status.textContent=data.stage;if(typeof data.percent==='number'){progress.value=data.percent;detail.textContent=`${data.percent}% complete`;}else{progress.removeAttribute('value');detail.textContent=data.bytes?`${(data.bytes/1048576).toFixed(0)} MB copied · Keep the launcher open`:'Your saves, settings and textures are being copied.';}});
        return await call('setup_migrate_install',{root:pending.installRoot,image:pending.image});
      }catch(error){loading.hidden=true;status.textContent='Import paused';detail.textContent=String(error.message||error);for(const node of [summary,overrides,note])node.hidden=false;throw error;}
      finally{unlisten?.();}

    }};
  },

  system_check: () => {
    const recommended=pending.hardware?.recommended;
    const presets=el('div','graphics-presets');
    const options=[{value:'performance',what:'Performance',why:'ReShade disabled. The lightest option.'},{value:'balanced',what:'Balanced',why:'ReShade enabled with lighter effects.'},{value:'quality',what:'All-Out',why:'ReShade enabled with high-end effects.'}];
    if(pending.graphicsOptions?.previous)options.push({value:'previous',what:'Previous settings',why:'Keep the graphics settings imported from your old installation.'});
    if(pending.graphicsMode==='previous')pending.preset='previous';
    const choices=radios('preset',options);
    for(const choice of choices){if(choice.querySelector('input')?.value===recommended){choice.classList.add('recommended');choice.append(el('span','recommendation-badge','Recommended'));}choice.addEventListener('change',()=>{pending.graphicsMode=pending.preset==='previous'?'previous':'preset';});}
    presets.append(...choices);
    return {nodes:[el('p',null,'Choose your ReShade effects. Required game settings are applied automatically.'),presets,el('p','hardware-note',pending.hardware?.description?`Your Specs: ${pending.hardware.description}`:'')],ready:()=>!!pending.preset,
      answer:async()=>{const preset=pending.preset==='previous'?'balanced':pending.preset;const result=await call('onboarding_choose_graphics',{preset});if(result.outcome==='refused')return result;await call('setup_save_graphics_options',{mode:pending.preset==='previous'?'previous':'preset',resolution:3,msaa:1,ssaa:false});try{localStorage.setItem('h:preset',preset);}catch(_){}return result;}};
  },

  controller_setup: () => {
    // A returning player's notice (the 2.1 B/X swap the launcher removed
    // from their mapping) takes the intro line's slot, so the step keeps its
    // height and no scrollbar appears. Adam, 2026-09-11: on onboarding and
    // Controllers only.
    const notices=el('div','notices setup-notices');
    const intro=el('p',null,'Test your inputs. Your controller can navigate the entire launcher once paired.');
    const nodes=[notices,intro];
    mountSetupNotices(notices,shown=>{intro.hidden=shown;});
    const toolbar=el('div','controller-picker');const select=el('select');select.setAttribute('aria-label','Your controller');
    const pads=pending.pads?.list||[];
    const placeholder=el('option',null,'Choose a controller');placeholder.value='';select.append(placeholder);
    pads.forEach((p,i)=>{const o=el('option',null,p.device_string.split('/').slice(2).join('/')||`${p.family} controller ${i+1}`);o.value=String(i);select.append(o);});
    const later=el('option',null,'Set up a controller later');later.value='none';select.append(later);select.value=pending.port??'';
    select.addEventListener('change',()=>{if(moving){select.value=pending.port??'';return;}pending.port=select.value||undefined;draw();});
    const scan=el('button',null,'Scan again');scan.type='button';scan.onclick=async()=>{if(moving)return;moving=true;scan.disabled=true;updateNav();await scanPads();if(pending.pads?.list?.length===1)pending.port='0';moving=false;draw();};
    toolbar.append(select,scan);nodes.push(toolbar);
    let workspace=null;const pad=pending.port!==undefined&&pending.port!=='none'?pads[Number(pending.port)]:null;
    if(pad&&window.createSetupController){workspace=window.createSetupController({device:pad.device_string,runtime:pending.runtime||null,onBusy:value=>{moving=value;updateNav();}});nodes.push(workspace);}
    else nodes.push(el('div','controller-empty',pending.port==='none'?'You can connect a controller from the Controllers page whenever you’re ready.':pending.pads?.error||'Connect your controller, press a button to wake it, then scan again.'));
    return {nodes,dispose:()=>workspace?.dispose(),ready:()=>pending.port!==undefined,
      answer:()=>pending.port==='none'?call('onboarding_decline_controller',{because:'controller setup deferred'}):call('onboarding_choose_controller',{port:Number(pending.port),expectedDevice:pad?.device_string||null,runtime:pending.runtime||null})};
  },

  controls_profile: () => {
    const opts = pending.controls;
    const nodes = [
      el(
        "p",
        null,
        "Choose the option that matches the controller you are using. You can change this at any time.",
      ),
    ];

    if (!opts) {
      nodes.push(el("p", "note", "Reading the profiles."));
    } else if (opts.error) {
      nodes.push(el("div", "refusal", opts.error));
    } else {
      // Suggested, never applied unsolicited. Someone on an Xbox pad may well
      // want GameCube art, since the game is a GameCube game and its own
      // tutorials draw it that way.
      if (pending.profile === undefined) {
        const selected=state.summary?.controller||'';
        const profile=/playstation|dualshock|dualsense|054c|wireless controller/i.test(selected)?'playstation':opts.list.find(o=>o.suggested&&o.slug!=='gamecube')?.slug||'xbox';
        pending.profile=profile;
      }
      const choices=el('div','prompt-choices');
      choices.append(
        ...radios(
          "profile",
          opts.list.map((o) => ({
            value: o.slug,
            what: o.label,
            why: '',
          })),
        ),
      );
      nodes.push(choices);

    }

    return {
      nodes,
      ready: () => !!pending.profile,
      answer: () => call("onboarding_choose_controls", { profile: pending.profile }),
    };
  },

  updates: () => {
    if (pending.automatic === undefined) pending.automatic = "yes";
    const choices=radios('automatic',[
      {value:'yes',what:'Keep Me Up To Date',why:'Downloads and installs verified updates automatically. Update summary after installation is complete.'},
      {value:'no',what:'Ask me first',why:'Nothing is downloaded until you say so.'}
    ]);
    choices[0].classList.add('recommended');choices[0].append(el('span','recommendation-badge','Recommended'));
    return {
      nodes: [
        el(
          "p",
          null,
          "Choose how Reborn handles new versions.",
        ),
        ...choices,
        el(
          "p",
          "note",
          "You can change this preference later in Settings.",
        ),
      ],
      ready: () => pending.automatic !== undefined,
      answer: () =>
        call("onboarding_choose_updates", { automatic: pending.automatic === "yes" || pending.automatic === true }),
    };
  },

  preparation: () => {
    const summary=state.summary||{};
    const list=el('div','recap-list');
    const names={performance:'Performance',balanced:'Balanced',quality:'All-Out',ultra:'All-Out',xbox:'Xbox',playstation:'PlayStation',gamecube:'GameCube'};
    const items=[
      ['game_verification','Game',(()=>{try{return localStorage.getItem('setup:patched:'+installation?.game)==='true'?'Patched':'Ready';}catch(_){return 'Ready';}})()],
      ['controller_setup','Controller',summary.controller?.split('/').slice(2).join('/')||summary.controller||'Set up later'],
      ['controls_profile','Button textures',names[summary.controls_profile]||summary.controls_profile||'Not chosen'],
      ['system_check','Graphics',pending.graphicsOptions?.current?.label||names[summary.graphics]||summary.graphics||'Not chosen'],
      ['updates','Updates',summary.automatic_updates?'Automatic downloads':'Ask me first'],
    ];
    for(const [stage,label,value] of items){const row=el('button','recap-row');row.type='button';row.append(el('span','recap-label',label),el('strong',null,value),el('span','recap-edit','Edit'));row.setAttribute('aria-label',`Edit ${label}: ${value}`);row.onclick=async()=>{if(moving)return;moving=true;reviewReturn=true;pending={};await refresh(stage);moving=false;updateNav();};list.append(row);}
    return {nodes:[el('p',null,'Everything look right? Select a setting to change it.'),list],ready:()=>true,nextLabel:'Continue',
      answer:async()=>{
        document.getElementById('hint').textContent='Applying';
        for(const task of ['write_config','check_everything']){
          const out=await call('onboarding_run_task',{task,runtime:pending.runtime||null,game:installation?.game||null});
          if(out?.step?.outcome==='refused')return out;
        }
        return call('onboarding_prepared');
      }};
  },

  // The game content (HD textures, roster, mods...) downloads here, as one
  // screen with one bar, before the player is told they are set. Before this
  // it came after setup as a small card on the Play page, and a Deck player
  // went into the game without textures (Adam, 2026-10-03: "a full downloading
  // game content screen apart of the onboarding").
  completion: () => {
    const dl=contentDownload();
    const broken='Your installation needs attention. Reopen setup or check Settings.';
    return {
      nodes:[dl.node],
      ready:()=>!!installation?.ready&&dl.finished(),
      get hint(){return installation?.ready?dl.hint():broken;},
      nextLabel:'Finish',
      dispose:()=>dl.dispose(),
      answer:async()=>{const result=await call('onboarding_finish',{confirm:true});if(result?.outcome!=='refused')window.location.replace('index.html');return result;},
    };
  },
};

const CONTENT_NAMES={textures:'HD textures',roster:'Roster',mods:'Mods','online-content':'Online content',runtime:'Emulator','runtime-linux':'Emulator','runtime-macos':'Emulator','game-patch':'Game update','game-import':'Game patch tools'};

function contentSize(bytes){
  const mb=(bytes||0)/1048576;
  return mb>=1024?`${(mb/1024).toFixed(2)} GB`:`${Math.max(0,Math.round(mb))} MB`;
}

function contentEta(seconds){
  if(!isFinite(seconds)||seconds<=0)return '';
  if(seconds<60)return 'less than a minute left';
  const m=Math.round(seconds/60);
  return m<60?`about ${m} min left`:`about ${Math.floor(m/60)} h ${m%60} min left`;
}

// One download, start to finish, drawn in place. Uses the same commands as
// the Play page's update card (content_check / content_fetch /
// content_promote), so a download that is already running is followed, not
// started twice.
function contentDownload(){
  let phase='checking',note='',alive=true,timer=null,samples=[],view=null;
  const root=el('div','content-download');
  const success=el('div','setup-success','✓');
  const lead=el('p','setup-intro','Checking for game content…');
  const bar=document.createElement('progress');bar.className='content-download-bar';bar.max=100;
  const amount=el('p','content-download-amount','');
  const rate=el('p','content-download-rate','');
  const list=el('ul','content-download-list');
  const problem=el('p','refusal','');problem.hidden=true;
  const actions=el('div','content-download-actions');
  const retry=el('button','primary','Try again');retry.type='button';retry.hidden=true;
  const later=el('button',null,'Download later');later.type='button';later.hidden=true;
  actions.append(retry,later);
  root.append(success,lead,bar,amount,rate,list,problem,actions);

  const title=t=>{const h=document.querySelector('#screen h1');if(h)h.textContent=t;};
  const rows=new Map();
  function drawList(components){
    list.replaceChildren();rows.clear();
    for(const c of components){
      const row=el('li','content-download-item');
      row.append(el('span','content-download-name',CONTENT_NAMES[c.component]||c.component),el('span','content-download-size',contentSize(c.bytes)),el('span','content-download-state','Waiting'));
      rows.set(c.component,row);list.append(row);
    }
  }
  function setRows(word,cls){for(const row of rows.values()){row.lastChild.textContent=word;row.className='content-download-item'+(cls?' '+cls:'');}}
  function render(){
    if(!alive)return;
    success.hidden=phase!=='done';
    bar.hidden=!(phase==='downloading'||phase==='installing'||phase==='checking');
    amount.hidden=rate.hidden=phase!=='downloading';
    list.hidden=!rows.size;
    problem.hidden=phase!=='problem';problem.textContent=note;
    retry.hidden=phase!=='problem';
    later.hidden=!(phase==='downloading'||phase==='problem');
    if(phase==='done'){title("You're all set");lead.textContent=rows.size?"Game content installed. Your settings are applied. You're ready to hit the streets!":"Your settings are applied. You're ready to hit the streets!";}
    else if(phase==='skipped'){title("You're all set");lead.textContent='Your settings are applied. The game content keeps downloading in the background; the Play page shows its progress.';}
    else {title('Downloading game content');lead.textContent=phase==='checking'?'Checking for game content…':phase==='installing'?'Installing…':phase==='problem'?'The download stopped.':'Keep the launcher open. This is a large download.';}
    if(phase==='checking'||phase==='installing')bar.removeAttribute('value');
    updateNav();
  }
  function track(s){
    view=s;
    const total=s.download_bytes||0,got=Math.min(total,s.fetched_bytes||0),now=performance.now();
    samples.push([now,got]);while(samples.length>2&&now-samples[0][0]>8000)samples.shift();
    const [t0,b0]=samples[0];const speed=now>t0?(got-b0)*1000/(now-t0):0;
    bar.value=total?Math.min(99,got*100/total):0;
    amount.textContent=`${contentSize(got)} of ${contentSize(total)} · ${total?Math.floor(got*100/total):0}%`;
    rate.textContent=speed>0?`${(speed/1048576).toFixed(1)} MB/s · ${contentEta((total-got)/speed)}`:'Starting…';
  }
  async function poll(){
    if(!alive||phase!=='downloading')return;
    try{const s=await call('content_status');if(s.phase==='fetching')track(s);}catch(_){}
    timer=setTimeout(poll,1000);
  }
  async function run(){
    phase='checking';note='';render();
    try{
      let s=await call('content_status');
      if(['idle','up_to_date','problem','installed'].includes(s.phase))s=await call('content_check',{depth:null});
      const wanted=(s.components||[]).filter(c=>c.state==='available');
      if(['available','fetching','staged','installing'].includes(s.phase)&&wanted.length)drawList(wanted);
      if(s.phase==='available'||s.phase==='fetching'){
        phase='downloading';samples=[];setRows('Downloading','active');render();track(s);timer=setTimeout(poll,1000);
        s=s.phase==='available'?await call('content_fetch'):s;
        while(alive&&s.phase==='fetching'){await new Promise(r=>setTimeout(r,1000));s=await call('content_status');}
        clearTimeout(timer);
      }
      if(!alive||phase==='skipped')return;
      if(s.phase==='staged'||s.phase==='installing'){
        phase='installing';bar.value=100;setRows('Installing','active');render();
        s=s.phase==='staged'?await call('content_promote',{confirm:true}):s;
        while(alive&&s.phase==='installing'){await new Promise(r=>setTimeout(r,1000));s=await call('content_status');}
      }
      if(!alive||phase==='skipped')return;
      if(s.phase==='installed'||s.phase==='up_to_date'||!wanted.length&&s.phase!=='available'){phase='done';setRows('Installed','done');}
      else {phase='problem';note=s.note||'The download did not finish. Try again; it resumes where it stopped.';}
    }catch(e){phase='problem';note=String(e?.message||e)+' Try again; it resumes where it stopped.';}
    render();
  }
  retry.onclick=()=>{if(phase==='problem')run();};
  later.onclick=()=>{phase='skipped';clearTimeout(timer);rows.clear();list.replaceChildren();render();};
  queueMicrotask(run);
  return {
    node:root,
    finished:()=>phase==='done'||phase==='skipped',
    hint:()=>phase==='downloading'?'Finish unlocks when the download is done, or choose Download later.':phase==='problem'?'Try again, or choose Download later.':'',
    dispose:()=>{alive=false;clearTimeout(timer);},
  };
}

function pathBox(key, placeholder) {
  const box = document.createElement("input");
  box.type = "text";
  box.placeholder = placeholder;
  box.value = pending[key] || "";
  box.spellcheck = false;
  box.style.width = "100%";
  box.addEventListener("input", () => {
    pending[key] = box.value.trim();
    updateNav();
  });
  box.setAttribute("aria-label", placeholder);
  const row = el("div", "path-entry");
  const browse = el("button", null, "Browse");
  browse.type = "button";
  browse.addEventListener("click", async () => {
    browse.disabled = true;
    try {
      const path = await call("pick_path", { kind: key === "image" ? "game" : "folder" });
      if (path) { box.value = path; pending[key] = path; updateNav(); }
    } catch (e) {
      showStepError(e);
    } finally { browse.disabled = false; }
  });
  row.appendChild(box); row.appendChild(browse);
  return row;
}

// ---------------------------------------------------------------------------
// The frame
// ---------------------------------------------------------------------------

let current = null;
let shownStage = null;

function draw() {
  current?.dispose?.();
  drawSteps();
  const host = document.getElementById("screen");

  if (state.complete || state.next.kind === "already_complete") {
    host.replaceChildren(
      el("h1", null, "Already set up"),
      el(
        "p",
        null,
        "Your setup is complete. You can change your preferences in Settings or return to Play.",
      ),
    );
    host.appendChild(installationPanel());
    const play = el("a", "button primary", "Back to Play");
    play.href = "index.html";
    host.appendChild(play);
    const restart = el("details", "block");
    restart.appendChild(el("summary", null, "Set up another installation"));
    restart.appendChild(el("p", "note", "This archives your setup progress and repeats the import and preparation steps. Review the installation location in Settings first."));
    const label = el("label", "choice");
    const check = document.createElement("input"); check.type = "checkbox";
    label.appendChild(check); label.appendChild(el("span", null, "I want to run setup again."));
    const button = el("button", "write", "Restart setup"); button.disabled = true;
    check.addEventListener("change", () => { button.disabled = !check.checked; });
    button.addEventListener("click", async () => {
      if (!check.checked) return;
      button.disabled = true;
      try { await call("onboarding_start_over", { confirm: true }); pending = {}; await refresh(); }
      catch (e) { fail(e); }
    });
    restart.appendChild(label); restart.appendChild(button); host.appendChild(restart);
    current = null;
    updateNav();
    return;
  }

  // Reconcile is NOT a different screen. It is this stage's own screen with a
  // warning on top: the stage writes files and a previous run stopped inside
  // it, so some of its work may already have happened.
  //
  // Built as a separate read-only notice at first, which made it a dead end.
  // Preparation enters that state the moment its tasks are recorded and the
  // stage is not yet answered, which is exactly where a player ends up, and
  // there was no button to carry them out of it.
  const reconciling = state.next.kind === "reconcile";

  const slug = state.next.stage;
  const build = SCREENS[slug];
  if (!build) {
    host.replaceChildren(
      el("h1", null, slug.replace(/_/g, " ")),
      el(
        "div",
        "refusal",
        `No screen is built for the ${slug} stage yet. The core is offering it, ` +
          "so this is a gap here and not a gap there.",
      ),
    );
    current = null;
    updateNav();
    return;
  }

  const changedStage = shownStage !== slug;
  shownStage = slug;
  document.body.dataset.setupStage=slug;
  current = build();
  host.replaceChildren(
    ...heading(titleFor(slug), state.next),
    // The core's own advice, above the screen rather than instead of it.

    ...current.nodes,
  );
  if(changedStage&&host.animate&&!matchMedia('(prefers-reduced-motion: reduce)').matches)host.animate([{opacity:0,transform:'translateY(6px)'},{opacity:1,transform:'translateY(0)'}],{duration:180,easing:'cubic-bezier(.2,.7,.2,1)'});
  updateNav();
  if (changedStage && typeof requestAnimationFrame === "function") requestAnimationFrame(() => {
    const title = host.querySelector('h1');
    title?.setAttribute('tabindex','-1');
    const target = document.body.classList.contains('gamepad-input')
      ? host.querySelector('input[type="radio"],input[type="checkbox"]') || document.getElementById('next')
      : title;
    target?.focus({preventScroll:true});
    document.getElementById('main').scrollTop = 0;
  });
}

function titleFor(slug) {
  if(slug === "game_verification" && installInfo?.mode === "fresh") return "Choose your game";
  return {
    welcome: "Welcome to NFL Street Reborn",
    migration_offer: "Keep your progress?",
    migration_copy: "Bringing it over",
    game_verification: "Make yourself at home",
    system_check: "Choose your graphics setting",
    controller_setup: "Your controller",
    controls_profile: "Which button textures?",
    updates: "Update Preferences",
    preparation: "Review your settings",
    completion: "You're all set",
  }[slug] || slug.replace(/_/g, " ");
}

function updateNav() {
  const next = document.getElementById("next");
  const back = document.getElementById("back");
  const hint = document.getElementById("hint");
  const nav = document.getElementById("nav");
  if (nav) nav.hidden = !!(state?.complete || state?.next?.kind === "already_complete");

  // Called, not read. ready() has to see the answer the player just gave, and
  // reading a boolean captured when the screen was built cannot.
  next.disabled = moving || !current || !current.ready();
  next.textContent = moving ? "Please wait…" : reviewReturn ? "Save and return" : (current && current.nextLabel) || "Continue";
  hint.textContent = (current && current.hint) || "";

  // Finish later waits too. Leaving mid-operation strands the work: the copy
  // keeps running with no page listening, and the re-shown step is refused
  // because a session is already active.
  const exit = typeof document.querySelector === "function" ? document.querySelector(".setup-exit") : null;
  if (exit) { exit.classList.toggle("disabled", moving); exit.setAttribute("aria-disabled", String(moving)); exit.tabIndex = moving ? -1 : 0; }

  // Back is asked, never assumed: the previous stage may be irreversible, in
  // which case there is no way back and the button should say so by being off.
  back.disabled = true;
  back.textContent=reviewReturn?"Back to review":"Back";
  if(reviewReturn){back.disabled=moving;return;}
  if (state && state.next && state.next.stage) {
    const requestedStage=state.next.stage;
    call("onboarding_back", { stage: requestedStage })
      .then((answer) => {
        if(reviewReturn||state?.next?.stage!==requestedStage)return;
        back.disabled = moving || !answer.to || (answer.to === 'welcome' && state.plan?.some(step => step.stage === 'game_verification' && step.satisfied));
        back.removeAttribute("title");
      })
      .catch(() => {});
  }
}

document.getElementById("next").addEventListener("click", async () => {
  if (moving || !current || !current.ready()) return;
  moving = true;
  document.querySelector(".step-error")?.remove();
  const btn = document.getElementById("next");
  updateNav();
  document.getElementById('screen').querySelectorAll('.refusal').forEach(n=>n.remove());
  try {
    const answer = await current.answer();
    // A refusal is not an error. It is the core saying no with a reason, and
    // the reason belongs on the screen the player is looking at.
    const step = answer && answer.step ? answer.step : answer;
    if (step && step.outcome === "refused") {
      document
        .getElementById("screen")
        .appendChild(el("div", "refusal", step.refused_because));
      moving = false; btn.disabled = false;
      return;
    }
    pending = {};
    const returnTo=reviewReturn?'preparation':null;reviewReturn=false;
    await refresh(returnTo);
  } catch (e) {
    showStepError(e);
  } finally { moving = false; updateNav(); }
});

document.getElementById("back").addEventListener("click", async () => {
  if (moving) return;
  if(reviewReturn){moving=true;reviewReturn=false;pending={};await refresh('preparation');moving=false;updateNav();return;}
  try {
    const answer = await call("onboarding_back", { stage: state.next.stage });
    if (!answer.to) return;
    pending = {};
    await call("onboarding_reach", { stage: answer.to });
    await refresh(answer.to);
  } catch (e) {
    fail(e);
  }
});

document.getElementById("clear")?.addEventListener("click", () => {
  document.getElementById("out").textContent = "";
});

if (!bridge) {
  document.getElementById("screen").replaceChildren(
    el("h1", null, "The core is not attached"),
    el(
      "div",
      "refusal",
      "This window loaded but the launcher core did not, so nothing can be " +
        "called. Nothing on screen would be real.",
    ),
  );
} else {
  refresh();
}

if(typeof document.addEventListener === "function") document.addEventListener("refresh-devices",async()=>{
 if(state?.next?.stage!=="controller_setup"||moving||document.body.dataset.controllerTesting)return;
 moving=true;updateNav();await scanPads();if(pending.pads?.list?.length===1)pending.port='0';moving=false;draw();
});
