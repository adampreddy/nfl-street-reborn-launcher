// Check that every shell page survives every answer the core can give.
//
// Play, Controls, Settings and Recovery each read several commands and draw
// from whatever came back. The failure this guards against is the one the
// first run screens had: a page that works when every read succeeds and
// throws when one field is absent. So each page is loaded three times, with
// a fake core that answers everything, answers with every field null, and
// rejects every call, and each time the page has to draw without throwing.
//
//     node Tools/check-shell-pages.mjs

import { readFileSync } from "fs";
import path from "node:path";

// ---- a DOM that records structure ------------------------------------------
function fakeEl(tag) {
  return {
    tagName: tag, children: [], className: "", textContent: "", attrs: {}, listeners: {}, dataset: {},
    append(...c) { this.children.push(...c); },
    replaceChildren(...c) { this.children = c; },
    addEventListener(k, f) { (this.listeners[k] ||= []).push(f); },
    setAttribute(k, v) { this.attrs[k] = v; },
    querySelectorAll() { return []; },
    querySelector() { return null; },
    // Enough for the Settings controls, which the checks below render.
    insertBefore(c, ref) { const i = this.children.indexOf(ref); this.children.splice(i < 0 ? this.children.length : i, 0, c); return c; },
    prepend(...c) { this.children.unshift(...c); },
    remove() {},
    focus() {},
    style: {},
    classList: { add() {}, remove() {}, toggle() {}, contains: () => false },
  };
}
const byId = {};
global.document = {
  body: {dataset:{}},
  dispatchEvent() {},
  getElementById: (id) => (byId[id] ||= fakeEl("div#" + id)),
  createElement: (tag) => fakeEl(tag),
  addEventListener() {},
  querySelectorAll: () => [],
};
global.location = { pathname: "/index.html" };
global.localStorage = { getItem: () => null, setItem() {}, removeItem() {} };
const tick = () => new Promise((r) => setTimeout(r, 0));

function text(node) {
  if (node === null || node === undefined) return "";
  if (typeof node === "string") return node;
  return [node.textContent || "", ...node.children.map(text)].join(" ");
}

// ---- answers shaped like the real commands ---------------------------------
const FULL = {
  launch_check: { ready: true, runtime: "C:\\rt\\Dolphin.exe", runtime_note: "Using the emulator that came with the launcher.", runtime_kind: "bundled", game: "C:\\g\\NSR.iso", game_note: "Using NSR.iso, found by name.", user_dir_note: "Using the user folder beside the emulator.", problem: null },
  devices: { pads: [{ device_string: "XInput/0/Gamepad", family: "xbox", port: 1, set_aside: null }], other: [], complete: false, unsearched: ["SDL"], source: "builtin" },
  mod_packages: { packages_root: "x", packages: [{ id: "a", version: "1", name: "A", active: true }], unreadable: [], activation: [{ id: "a", version: "1" }], exclusive: [], managed_by_controls: [] },
  dolphin_preferences: {values:{},editable:true},
  controller_slots: {slots: null, primary: null},
  controls_current: { profile: "xbox", label: "Xbox", source: "settings" },
  controls_options: [{ slug: "xbox", label: "Xbox", describes: "d", pack: "p", suggested: true }],
  home_state: { sessions_played: 3, minutes_played: 12, last_played: "id", last_played_ms: 1756800000000, controllers: 1, mods_installed: 9, mods_applied: 2, runtime_set: true },
  onboarding_progress: { complete: true, summary: { graphics: "balanced" } },
  paths: { install_root: "i", data_root: "d", saves_survive_uninstall: true, directories: [{ relative: "mods", exists: true, class: "managed" }], channel: "stable", channel_note: "stable channel." },
  identity_status: { signed_in: true, display_name: "Adam", player_id: "p1", show_offline: false },
  stats_queue: { dir: "q", depth: 2, quarantined: 0, stalled: 0, accepted: 0, held: 0, rejected: 0, total: 2, orphaned_payloads: 0, entries: [] },
  content_status: { phase: "available", origin: "https://files.test/", release: "3.0.0 sequence 3", components: [{ component: "textures", state: "available", installed: "1.0.0", offered: "1.0.1", files: 3, bytes: 4194304, repair: false, message: null }], download_bytes: 4194304, download_files: 3, repair_only: false, note: "Update available: 4 MB across 3 file(s).", installed: null },
  sessions: [{ id: "s1", build: "offline", minutes: 3.2, ended: "Crashed", needs_recovery: true }, { id: "s2", build: "offline", minutes: 1, ended: "Exited", needs_recovery: true }, { id: "s3", build: "offline", minutes: 1, ended: "Terminated", needs_recovery: true }],
};
// Every field null, and every list answer null too: a list that is absent is
// not an empty list, and a page that assumes it is throws on .length.
const NULLS = Object.fromEntries(Object.entries(FULL).map(([k, v]) => [k, Array.isArray(v) ? null : Object.fromEntries(Object.keys(v).map((f) => [f, null]))]));

let fails = 0;
function check(what, ok, detail = "") {
  if (!ok) fails++;
  console.log(`${ok ? "ok  " : "FAIL"}  ${what}${ok ? "" : `: ${detail}`}`);
}

// A data: URL cannot resolve relative imports. Keep each production module's
// scope and exports intact, recursively replacing imports with data: URLs.
// A fresh graph per page also keeps shell.js's bridge tied to that page's core.
function pageModule(file, nonce = Math.random(), modules = new Map()) {
  file = path.resolve(file);
  if (modules.has(file)) return modules.get(file);
  const src = readFileSync(file, "utf8").replace(
    /(\bfrom\s*['"])(\.\.?\/[^'"]+)(['"])/g,
    (_, before, relative, after) => before + pageModule(path.resolve(path.dirname(file), relative), nonce, modules) + after,
  );
  const url = "data:text/javascript;base64," + Buffer.from(src + `\n// ${file} ${nonce}`).toString("base64");
  modules.set(file, url);
  return url;
}

async function loadPage(file, answers, mode) {
  const calls = [];
  global.window = {
    addEventListener() {},
    __TAURI__: {
      core: {
        invoke: async (cmd) => {
          calls.push(cmd);
          if (mode === "reject") throw new Error(`${cmd} failed on purpose`);
          if (!(cmd in answers)) throw new Error(`no fake answer for ${cmd}`);
          return answers[cmd];
        },
      },
    },
  };
  byId["screen"] = fakeEl("div#screen");
  await import(pageModule(file));
  await tick(); await tick(); await tick();
  return { calls, screen: byId["screen"] };
}

// A loader that hands the page its own `invoke` and a clock it does not own,
// so a timer-driven flow can be stepped tick by tick instead of waited on.
async function loadPageDriven(file, invoke) {
  const timers = [];
  const realInterval = global.setInterval;
  const realClear = global.clearInterval;
  global.setInterval = (fn) => timers.push(fn);
  global.clearInterval = (id) => {
    if (id) timers[id - 1] = null;
  };
  global.window = { addEventListener() {}, __TAURI__: { core: { invoke } } };
  byId["screen"] = fakeEl("div#screen");
  await import(pageModule(file));
  await tick(); await tick(); await tick();
  return {
    screen: () => byId["screen"],
    // Fired, not awaited. A real interval does not wait for its own callback,
    // and the whole point of this check is what happens while one is still out.
    async step() {
      for (const fn of timers) if (fn) fn();
      await tick(); await tick(); await tick();
    },
    done() {
      global.setInterval = realInterval;
      global.clearInterval = realClear;
    },
  };
}

function findByAttr(node, key, wanted) {
  if (!node || typeof node !== "object") return null;
  if (node.attrs && node.attrs[key] === wanted) return node;
  for (const child of node.children || []) {
    const hit = findByAttr(child, key, wanted);
    if (hit) return hit;
  }
  return null;
}

function findByText(node, wanted) {
  if (!node || typeof node !== "object") return null;
  if ((node.textContent || "") === wanted) return node;
  for (const child of node.children || []) {
    const hit = findByText(child, wanted);
    if (hit) return hit;
  }
  return null;
}

const PAGES = [
  ["crates/reborn-ui/ui/home.js", "Play"],
  ["crates/reborn-ui/ui/controls.js", "Controls"],
  ["crates/reborn-ui/ui/settings.js", "Settings"],
  ["crates/reborn-ui/ui/stats.js", "Stats"],
  ["crates/reborn-ui/ui/recovery.js", "Recovery"],
];

for (const [file, name] of PAGES) {
  for (const mode of ["full", "nulls", "reject"]) {
    const answers = mode === "nulls" ? NULLS : FULL;
    try {
      const { calls, screen } = await loadPage(file, answers, mode);
      const drawn = text(screen).trim().length > 0;
      const how = mode === "full" ? "answers everything" : mode === "nulls" ? "answers with every field null" : "rejects every call";
      check(`${name} draws when the core ${how}`, drawn, `nothing drawn; calls: ${calls.join(",")}`);
      if (mode === "reject") {
        check(`${name} says what failed`, /failed on purpose|could not|not loaded/i.test(text(screen)), text(screen).slice(0, 200));
      }
    } catch (e) {
      check(`${name} survives mode ${mode}`, false, e.message.slice(0, 200));
    }
  }
}

// Play: the facts it states come from the answers, not from defaults.
{
  const { screen } = await loadPage("crates/reborn-ui/ui/home.js", FULL, "full");
  const t = text(screen);
  check("Play names the image and why", t.includes("NSR.iso") && t.includes("found by name"));
  check("Play names the user folder", t.includes("beside the emulator"));
  check("Play says a pad list may be incomplete when it is", t.includes("Not every backend"));
  check("Play shows who is signed in", t.includes("Signed in as Adam"));
  // The league site is parked and its results endpoint is unbuilt; a toggle a
  // player could tick that then does nothing is worse than no toggle at all.
  check("Play does not offer the league toggle", !t.includes("This is a league match"));
}
// Play: a read that fails must never put the command's own name on the account
// card. In reject mode every call throws an Error whose message is the command
// name, which is the exact shape of the real leak: the card rendered
// `id.error` verbatim, so a locked Windows credential vault put the vault's own
// wording on the Play screen.
{
  const { screen } = await loadPage("crates/reborn-ui/ui/home.js", FULL, "reject");
  const t = text(screen);
  check("Play never names a command on the account card", !t.includes("identity_status"), t.slice(0, 300));
  check("Play says what to do when the account cannot be read", t.includes("Try again in a moment"), t.slice(0, 300));
}
// Recovery: three bad endings put mods-disabled first, in words.
{
  const { screen } = await loadPage("crates/reborn-ui/ui/recovery.js", FULL, "full");
  const t = text(screen);
  check("Recovery shows the last session", t.includes("Terminated") || t.includes("Session status"));
  check("Recovery advises mods-disabled after three bad endings", t.includes("Try starting with mods disabled"));
}
// Settings starts with everyday options; file and update controls have their own sections.
{
 const {screen}=await loadPage("crates/reborn-ui/ui/settings.js",FULL,"full");const t=text(screen);
 check("Settings exposes Dolphin and grouped preferences",t.includes("Open Dolphin")&&t.includes("Audio")&&t.includes("Saves"));
 // Patch notes and uninstall live on the Updates tab, which this harness
 // cannot select; the real window covers those.
 check("Settings no longer carries the account block", !t.includes("Sign out"));
}
// The account and results moved to their own page on 2026-09-10.
{
 const {screen}=await loadPage("crates/reborn-ui/ui/stats.js",FULL,"full");const t=text(screen);
 check("Stats has the account block", t.includes("Signed in as Adam") && t.includes("Sign out"));
 check("Stats offers to send results", t.includes("Send results now"));
}
{
 const message='Could not check whether Dolphin is open. Try again.';
 const answers={...FULL,dolphin_preferences:{...FULL.dolphin_preferences,editable:false,readonly_reason:message}};
 const {screen}=await loadPage("crates/reborn-ui/ui/settings.js",answers,"full");
 check("Settings explains why editing is unavailable",text(screen).includes(message));
}

// The graphics and audio choices are the platform's, sent by the launcher.
// Direct3D and WASAPI exist only on Windows; a Linux page offering them would
// write a backend Dolphin there cannot run. A launcher that sends no choices
// is a Windows one and keeps the Windows list.
{
 global.location = { pathname: "/index.html", hash: "#graphics" };
 const values={video_backend:"Vulkan",internal_resolution:"3",msaa:"1",sync_gpu:"False",vsync:"False",show_fps:"False"};
 const linux={...FULL,dolphin_preferences:{values,editable:true,os:"linux",choices:{video_backend:["Vulkan","OGL"],audio_backend:["Cubeb"]}}};
 let t=text((await loadPage("crates/reborn-ui/ui/settings.js",linux,"full")).screen);
 check("Linux offers Vulkan as the Reborn default", t.includes("Vulkan (Reborn default)") && t.includes("OpenGL"), t.slice(0,600));
 check("Linux never offers Direct3D or promises ReShade", !t.includes("Direct3D") && !t.includes("ReShade") && !t.includes("Visual effects"), t.slice(0,600));
 const windows={...FULL,dolphin_preferences:{values:{...values,video_backend:"D3D"},editable:true}};
 t=text((await loadPage("crates/reborn-ui/ui/settings.js",windows,"full")).screen);
 check("Windows keeps its four backends", t.includes("Direct3D 11 (Reborn default)") && t.includes("Direct3D 12") && t.includes("Vulkan") && t.includes("OpenGL"), t.slice(0,600));
 check("Windows keeps the visual effects presets", t.includes("Visual effects"), t.slice(0,600));
 global.location = { pathname: "/index.html", hash: "#audio" };
 const audio={volume:"100",muted:"False",audio_stretch:"False",audio_latency:"20",audio_backend:"Cubeb"};
 t=text((await loadPage("crates/reborn-ui/ui/settings.js",{...FULL,dolphin_preferences:{values:audio,editable:true,os:"linux",choices:{video_backend:["Vulkan","OGL"],audio_backend:["Cubeb"]}}},"full")).screen);
 check("Linux does not offer Windows exclusive audio", t.includes("Default (shared audio)") && !t.includes("Windows exclusive"), t.slice(0,600));
 global.location = { pathname: "/index.html" };
}

// Files & storage is where the launch refusal and the Play badge both send a
// player whose game file is not a Reborn build. It used to offer a path box, a
// Browse button and nothing that could repair anything, so a refused player
// arrived at a dead end.
{
 global.location = { pathname: "/index.html", hash: "#files" };
 const answers={...FULL,launch_check:{...FULL.launch_check,game_verdict:"unconverted"}};
 const {screen}=await loadPage("crates/reborn-ui/ui/settings.js",answers,"full");
 const t=text(screen);
 check("Settings offers a repair for a disc that is not a Reborn build", t.includes("Repair my game"), t.slice(0,400));
 check("Settings words that as a fault, never as a preference", t.includes("isn't NFL Street Reborn") && !/custom/i.test(t), t.slice(0,400));
 global.location = { pathname: "/index.html" };
}

// The Tier 2 choice, driven: choosing an unconverted image in Settings must
// put the choice to the player with Repair primary, and must NOT enrol them in
// the escape hatch on its own. `acknowledgeUnverified` may only ever leave the
// page on the call the second button makes.
{
 global.location = { pathname: "/index.html", hash: "#files" };
 const seen=[];
 const CHOICE={headline:"Your game file isn't NFL Street Reborn.",detail:"This is NFL Street 2, but not a Reborn build, so rosters, faces and stats won't match. This usually means setup couldn't convert your disc.",repair:"Repair my game",play_anyway:"Play it anyway"};
 const invoke=async(cmd,args)=>{
   seen.push([cmd,args||{}]);
   if(cmd==="settings_set_path"){
     if(args&&args.acknowledgeUnverified===true)return {which:args.which,value:args.value};
     return {which:args.which,value:null,unverified:CHOICE};
   }
   if(cmd in FULL)return FULL[cmd];
   throw new Error(`no fake answer for ${cmd}`);
 };
 const page=await loadPageDriven("crates/reborn-ui/ui/settings.js",invoke);
 const press=async node=>{ for(const fn of node.listeners.click||[]) await fn(); await tick(); await tick(); await tick(); };
 const saveButton=findByAttr(page.screen(),"aria-label","Save Game image");
 if(!saveButton){check("Settings has a Save button for the game image",false);}
 else{
   await press(saveButton);
   const t=text(page.screen());
   check("Choosing an unconverted image puts the choice to the player", t.includes(CHOICE.headline)&&t.includes("Repair my game")&&t.includes("Play it anyway"), t.slice(0,400));
   check("Nothing is acknowledged before the player answers", !seen.some(([cmd,args])=>cmd==="settings_set_path"&&args.acknowledgeUnverified===true), JSON.stringify(seen.filter(([c])=>c==="settings_set_path")));
   const anyway=findByText(page.screen(),"Play it anyway");
   if(!anyway){check("The escape hatch is offered",false);}
   else{
     await press(anyway);
     check("Play it anyway is the only thing that acknowledges", seen.some(([cmd,args])=>cmd==="settings_set_path"&&args.acknowledgeUnverified===true), JSON.stringify(seen.filter(([c])=>c==="settings_set_path")));
   }
 }
 page.done();
 global.location = { pathname: "/index.html" };
}

// The sign-in poll, driven tick by tick.
//
// `identity_sign_in_poll` does not answer until the sign-in is saved AND the
// whole backlog has been sent, which is exactly the slow case this feature
// exists for. The interval had an async body and no in-flight guard, so a
// later tick fired with a device code the server had already consumed, and the
// failure it got overwrote the screen. Success never cleared that text, so the
// card could read "Signed in as Adam" with "The sign-in code expired. Try
// again." underneath it.
{
  let pollCalls = 0;
  let signedIn = false;
  let finishFirstPoll;
  const invoke = async (cmd) => {
    if (cmd === "identity_status") {
      return signedIn
        ? { signed_in: true, display_name: "Adam", player_id: "p1", show_offline: false }
        : { signed_in: false, display_name: null, player_id: null, show_offline: null };
    }
    if (cmd === "identity_sign_in_start") {
      return { device_code: "dc", user_code: "ABCD-EFGH", verify_url: "https://b/link", interval: 1, opened: true };
    }
    if (cmd === "identity_sign_in_poll") {
      pollCalls += 1;
      // The first one is the slow one: it is still out when the next tick fires.
      if (pollCalls === 1) {
        return new Promise((resolve) => {
          finishFirstPoll = () => { signedIn = true; resolve({ state: "done", display_name: "Adam" }); };
        });
      }
      // Anything after it is asking about a code the server has consumed.
      throw new Error("The sign-in code expired. Try again.");
    }
    if (cmd in FULL) return FULL[cmd];
    throw new Error(`no fake answer for ${cmd}`);
  };

  const page = await loadPageDriven("crates/reborn-ui/ui/home.js", invoke);
  const button = findByText(page.screen(), "Sign in with Discord");
  check("Play offers a sign-in button when signed out", !!button);
  if (button) {
    await button.listeners.click[0]();
    await tick(); await tick();
    await page.step();
    await page.step();
    check("Sign-in polls never overlap", pollCalls === 1, `${pollCalls} polls went out while one was still in flight`);
    if (finishFirstPoll) {
      finishFirstPoll();
      await tick(); await tick(); await tick(); await tick();
    }
    const t = text(page.screen());
    check("A finished sign-in is shown", t.includes("Signed in as Adam"), t.slice(0, 300));
    check("A finished sign-in leaves no failure text beside it", !t.includes("expired"), t.slice(0, 300));
  }
  page.done();
}

// The results note, after a game, while signed out.
//
// The account card only drew `result.stats_note` inside the signed-in branch,
// so the one sentence written for a signed-out player -- their game was
// recorded and is waiting for them to sign in -- was never displayed to the
// only person it was for.
{
  const NOTE = "Match results: 1 result recorded, waiting for sign-in.";
  const invoke = async (cmd) => {
    if (cmd === "identity_status") {
      return { signed_in: false, display_name: null, player_id: null, show_offline: null };
    }
    if (cmd === "launch") {
      return { started: true, message: "Session ended after 12 minutes.", minutes: 12, needs_recovery: false, pads: [], pads_authoritative: true, mods_note: null, session_id: "s1", user_dir_note: "u", stats_note: NOTE };
    }
    if (cmd in FULL) return FULL[cmd];
    throw new Error(`no fake answer for ${cmd}`);
  };
  const page = await loadPageDriven("crates/reborn-ui/ui/home.js", invoke);
  const play = findByAttr(page.screen(), "id", "play-now");
  check("Play offers Play now when the checks are ready", !!play);
  if (play) {
    await play.listeners.click[0]();
    await tick(); await tick(); await tick();
    const t = text(page.screen());
    check("A signed-out player is told their results are waiting", t.includes(NOTE), t.slice(0, 300));
  }
  page.done();
}

console.log(fails === 0 ? "\nPASS: every shell page survives every answer" : `\nFAIL: ${fails}`);
process.exit(fails ? 1 : 0);
