// Check that answering a first run screen enables its Next button.
//
// Why this exists. Every screen returns a `ready` telling the frame whether
// Next can fire. It was a plain boolean, computed once when the screen was
// built. Clicking a radio updated the pending answer and called updateNav, and
// updateNav re-read a value that could never change, so Next stayed disabled
// forever on every screen that asks a question. Adam hit it on migration and
// could not get past step 3 of 9.
//
// So `ready` is a function now, and this proves it is called rather than read.
// Clicking is the only other way to find this out, and by then it has shipped.
//
//     node Tools/check-onboarding-ready.mjs

import { readFileSync } from "fs";
let src = readFileSync("crates/reborn-ui/ui/onboarding.js", "utf8");

const fake = () => new Proxy(
  { style:{}, classList:{add(){},remove(){}}, value:"", textContent:"", disabled:false, title:"" },
  { get:(t,k)=> k in t ? t[k] : ()=>fake(), set:(t,k,v)=>((t[k]=v),true) });
global.localStorage = { getItem:()=>null, setItem(){}, removeItem(){} };
global.document = { getElementById:()=>fake(), createElement:()=>fake() };
global.window = {};   // no __TAURI__, so bridge is null and refresh() never runs

const mod = src + "\nexport { SCREENS, __set };\nfunction __set(s,p){ state=s; pending=p; }";
const { SCREENS, __set } = await import(
  "data:text/javascript;base64," + Buffer.from(mod).toString("base64"));

// Adam's exact state, from the journal and the screenshot.
const state = {
  complete: false,
  next: { kind:"show", stage:"migration_offer", position:3, total:9, resumed:true },
  plan: [], set_aside: [], summary:{ absent:[] },
  legacy_root_checked: "C:\Users\Adam\AppData\Roaming\NFL Street Reborn",
  migration_blocked: null,
  preparation_remaining: [], tasks_done: [],
};

let fails = 0;
function check(what, got, want) {
  const ok = got === want;
  if (!ok) fails++;
  console.log(`${ok ? "ok  " : "FAIL"}  ${what}: ready()=${got}, expected ${want}`);
}

// Nothing chosen yet: Next must be off.
let pending = {};
__set(state, pending);
check("migration, nothing selected", SCREENS.migration_offer().ready(), false);

// "Start fresh". This is the click that did nothing.
pending = { migration: "no" };
__set(state, pending);
check("migration, Start fresh", SCREENS.migration_offer().ready(), true);

pending = { migration: "yes" };
__set(state, pending);
check("migration, Bring it over", SCREENS.migration_offer().ready(), true);

// Blocked: declining must STILL be allowed, accepting must not.
const blocked = { ...state, migration_blocked: "19 paths have no backup" };
__set(blocked, { migration: "no" });
check("blocked, Start fresh", SCREENS.migration_offer().ready(), true);
__set(blocked, { migration: "yes" });
check("blocked, Bring it over", SCREENS.migration_offer().ready(), false);

// The same fault would have frozen these two.
__set({ ...state, next:{ ...state.next, stage:"system_check" } }, {});
check("graphics, nothing selected", SCREENS.system_check().ready(), false);
__set({ ...state, next:{ ...state.next, stage:"system_check" } }, { preset:"balanced" });
check("graphics, balanced selected", SCREENS.system_check().ready(), true);

__set(state, {});
check("controller, nothing selected", SCREENS.controller_setup().ready(), false);
__set(state, { port: "0" });
check("controller, port 1 selected", SCREENS.controller_setup().ready(), true);
__set(state, { port: "none" });
check("controller, declined", SCREENS.controller_setup().ready(), true);

// The controls profile, which replaced mod selection in that slot.
const cs = { ...state, next: { ...state.next, stage: "controls_profile" } };
__set(cs, {});
check("controls, list not loaded", SCREENS.controls_profile().ready(), false);

// A suggestion preselects, so the common case is ready with no click. Building
// the screen is what sets it, which is the point: a suggestion is a
// preselection and never an application.
const opts = {
  list: [
    { slug: "xbox", label: "Xbox", describes: "x", pack: "jsgme.xbox-controls", suggested: true },
    { slug: "playstation", label: "PS", describes: "y", pack: "jsgme.ps-controls", suggested: false },
    { slug: "gamecube", label: "GameCube", describes: "z", pack: null, suggested: false },
  ],
};
__set(cs, { controls: opts });
SCREENS.controls_profile();
check("controls, suggestion preselects", SCREENS.controls_profile().ready(), true);

// GameCube, the profile that applies nothing, is still a valid answer.
__set(cs, { controls: opts, profile: "gamecube" });
check("controls, GameCube chosen", SCREENS.controls_profile().ready(), true);

// A failed read must not leave Next clickable on a screen with no options.
__set(cs, { controls: { error: "could not read the profiles" } });
check("controls, read failed", SCREENS.controls_profile().ready(), false);

// With no pad there is no suggestion, so nothing is preselected and the player
// has to choose. Xbox being the default must not be dressed up as a
// recommendation nothing measured.
const noSuggestion = { list: opts.list.map((o) => ({ ...o, suggested: false })) };
__set(cs, { controls: noSuggestion });
SCREENS.controls_profile();
check("controls, no pad so no suggestion", SCREENS.controls_profile().ready(), false);


console.log(fails === 0 ? "\nPASS: a selection now enables Next" : `\nFAIL: ${fails}`);
process.exit(fails ? 1 : 0);
