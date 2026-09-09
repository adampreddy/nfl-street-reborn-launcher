// Check that the Mods page draws the rules it is meant to draw.
//
// Why this exists. The first run screens crashed the moment they were finally
// reachable, on a field the view no longer had. The Mods page has more shape
// than any of those screens: pick-one groups, a pair hidden because it is a
// setting, entries listed as on with nothing behind them, packages that could
// not be read. Every one of those is a real state on this machine, and a
// harness row cannot show what the page does with them. This can, before a
// click does.
//
//     node Tools/check-mods-page.mjs
//
// Set MODS_JS to check a different file, which is how the checker itself is
// checked: point it at a copy with a guard removed and it must fail.

import { readFileSync } from "fs";

const file = process.env.MODS_JS || "crates/reborn-ui/ui/mods.js";
const src = readFileSync(file, "utf8");

// A DOM that records structure, so assertions can read it back.
function fakeEl(tag) {
  return {
    tagName: tag,
    style: {},
    children: [],
    className: "",
    textContent: "",
    attrs: {},
    listeners: {},
    append(...c) {
      this.children.push(...c);
    },
    replaceChildren(...c) {
      this.children = c;
    },
    addEventListener(k, f) {
      (this.listeners[k] ||= []).push(f);
    },
    setAttribute(k, v) {
      this.attrs[k] = v;
    },
    querySelectorAll() {
      return [];
    },
  };
}
const byId = {};
global.document = {
  getElementById: (id) => (byId[id] ||= fakeEl("div#" + id)),
  createElement: (tag) => fakeEl(tag),
};

// The core, faked. Every answer is the shape the real command returns.
const calls = [];
let answers = {};
global.window = {
  __TAURI__: {
    core: {
      invoke: async (cmd, args) => {
        calls.push([cmd, args]);
        if (!(cmd in answers)) throw new Error(`no fake answer for ${cmd}`);
        const a = answers[cmd];
        if (a instanceof Error) throw a;
        return typeof a === "function" ? a(args) : a;
      },
    },
  },
};

function text(node) {
  if (node === null || node === undefined) return "";
  if (typeof node === "string") return node;
  return [node.textContent || "", ...node.children.map(text)].join(" ");
}
function walk(node, out = []) {
  if (!node || typeof node === "string") return out;
  out.push(node);
  node.children.forEach((c) => walk(c, out));
  return out;
}
const tick = () => new Promise((r) => setTimeout(r, 0));

let fails = 0;
function check(what, ok, detail = "") {
  if (!ok) fails++;
  console.log(`${ok ? "ok  " : "FAIL"}  ${what}${ok ? "" : `: ${detail}`}`);
}

// ---------------------------------------------------------------------------
// Fixtures shaped like this machine
// ---------------------------------------------------------------------------

const BALL = "GN7E69/tex1_128x128_m_ffc5151a141db330_14.dds";
const footballs = ["aqua", "blue", "camo", "green", "panda", "pink", "purple"].map((c) => ({
  id: `jsgme.${c}-football`,
  version: "1.0.0",
  name: `${c[0].toUpperCase()}${c.slice(1)} Football`,
  targets: ["GN7E69"],
  conflict_group: null,
  files: 1,
  bytes: 21_000,
  active: c === "purple",
}));
const controls = ["xbox", "ps"].map((c) => ({
  id: `jsgme.${c}-controls`,
  version: "1.0.0",
  name: `${c.toUpperCase()} Controls`,
  targets: ["GN7E69"],
  conflict_group: null,
  files: 19,
  bytes: 400_000,
  active: c === "xbox",
}));
const standalone = {
  id: "jsgme.saints-alternate",
  version: "1.0.0",
  name: "Saints Alternate",
  targets: ["GN7E69"],
  conflict_group: null,
  files: 4,
  bytes: 90_000,
  active: false,
};

// Two soundtrack packages: read raw by the window (schema nsr.soundtrack-mod.v1),
// never composed, switched through the soundtrack record.
const soundtracks = [
  { id: "reborn.soundtrack.original", version: "1.0.0", name: "NFL Street 2 Original Soundtrack", targets: [], conflict_group: "soundtrack", files: 1, bytes: 90, active: true, kind: "soundtrack" },
  { id: "reborn.soundtrack.nfl-street-1", version: "1.0.0", name: "NFL Street 1 Soundtrack", targets: [], conflict_group: "soundtrack", files: 1, bytes: 78_894_680, active: false, kind: "soundtrack" },
];
const library = {
  packages_root: "C:\\data\\mods\\packages",
  packages: [...footballs, ...controls, standalone, ...soundtracks],
  unreadable: ["C:\\data\\mods\\packages\\broken\\1.0.0: package.json is not a readable manifest"],
  activation: [
    { id: "jsgme.purple-football", version: "1.0.0" },
    { id: "jsgme.xbox-controls", version: "1.0.0" },
    { id: "jsgme.ghost-football", version: "1.0.0" },
  ],
  exclusive: [
    { members: footballs.map((f) => f.id), example_path: BALL, computed: true },
    { members: controls.map((c) => c.id), example_path: "GN7E69/btn_a.png", computed: true },
  ],
  managed_by_controls: controls.map((c) => c.id),
};
const options = [
  { slug: "xbox", label: "Xbox", describes: "A, B, X, Y.", pack: "jsgme.xbox-controls", suggested: true },
  { slug: "playstation", label: "PlayStation", describes: "Cross, Circle.", pack: "jsgme.ps-controls", suggested: false },
  { slug: "gamecube", label: "GameCube", describes: "The disc's own.", pack: null, suggested: false },
];
const current = { profile: "xbox", label: "Xbox", source: "settings" };
const syncDone = {
  game: "GN7E69",
  synced: true,
  refusal: null,
  note: "2 mod(s) on, tree rebuilt, 8059 entries, texture folder linked.",
  active: [],
  missing: [{ id: "jsgme.ghost-football", version: "1.0.0" }],
  tree: "rebuilt",
  entries: 8059,
  overlaid: 20,
  link: "linked",
  link_at: "C:\\User\\Load\\Textures\\GN7E69",
  moved_aside: "C:\\User\\Load\\Textures\\GN7E69.before-launcher-1",
  composed_root: "C:\\data\\mods\\active\\GN7E69",
};

answers = {
  mod_packages: library,
  controls_options: options,
  controls_current: current,
  mods_toggle: (args) => (args && String(args.id).startsWith("reborn.soundtrack.") ? { ...syncDone, soundtrack_note: "Your soundtrack is on the disc: 14 custom songs." } : syncDone),
  mods_sync: syncDone,
  controls_set: { profile: "playstation", synced: null, sync_note: "Recorded. Applied at the next launch: Another launcher is running" },
};

// ---------------------------------------------------------------------------
// Load the page. The bridge is present, so it reads on boot.
// ---------------------------------------------------------------------------

const mod =
  src +
  "\nexport { draw, __set, __state };" +
  "\nfunction __set(s) { if ('library' in s) library = s.library; if ('options' in s) options = s.options;" +
  " if ('current' in s) current = s.current; if ('lastSync' in s) lastSync = s.lastSync;" +
  " if ('busy' in s) busy = s.busy; if ('problem' in s) problem = s.problem; if ('notice' in s) notice = s.notice; }" +
  "\nfunction __state() { return { library, options, current, lastSync, busy, problem, notice }; }";
const { draw, __set, __state } = await import("data:text/javascript;base64," + Buffer.from(mod).toString("base64"));
await tick();
await tick();

const screen = () => byId["screen"];
const sections = (title) =>
  walk(screen()).filter((n) => n.tagName === "section" && n.children.some((c) => c.tagName === "h2" && c.textContent === title));
const inputs = (node) => walk(node).filter((n) => n.tagName === "input");

// Store behavior, using the same catalog and core mutation contract.
check("store starts with installed footballs",walk(screen()).filter(n=>n.className==="store-card").length===7);
check("button texture packs stay in Controllers",!walk(screen()).some(n=>n.attrs["aria-label"]==="XBOX Controls"));
check("stale activations remain removable",text(screen()).includes("jsgme.ghost-football"));
check("unreadable packages remain visible",text(screen()).includes("not a readable manifest"));
const enable=walk(screen()).find(n=>n.attrs["aria-label"]==="Enable Aqua Football");
await enable.listeners.click[0]();
{
  const tab = walk(screen()).find((n) => n.tagName === "button" && n.textContent === "Soundtrack");
  tab.listeners.click[0]();
  check("the Soundtrack tab lists the soundtrack packages", walk(screen()).filter((n) => n.className.split(" ").includes("store-card")).length === 2 && text(screen()).includes("NFL Street 1 Soundtrack"));
  const enable = walk(screen()).find((n) => n.attrs["aria-label"] === "Enable NFL Street 1 Soundtrack");
  check("a soundtrack package offers Enable", !!enable);
  await enable.listeners.click[0]();
  await tick(); await tick(); await tick();
  check("switching a soundtrack package shows what the disc rebuild did", text(screen()).includes("14 custom songs"));
  const back = walk(screen()).find((n) => n.tagName === "button" && n.textContent === "Footballs");
  back.listeners.click[0]();
}
check("enable sends the package identity",calls.some(([cmd,args])=>cmd==="mods_toggle"&&args.id==="jsgme.aqua-football"&&args.on===true));
for(const [label,value] of [["missing",null],["empty",{packages:[],activation:[]}],["null fields",{packages:null,activation:null,exclusive:null,unreadable:null,managed_by_controls:null}]]){
 __set({library:value,problem:null,notice:null});try{draw();check("store survives "+label,true);}catch(e){check("store survives "+label,false,e.message);}
}
check("empty store offers import",text(screen()).includes("Import mods"));
__set({library,problem:"Could not apply the selected mod."});draw();
check("mutation failures are displayed",text(screen()).includes("Could not apply the selected mod."));
console.log(fails===0?"PASS: mod store preserves core actions and failure states":`FAIL: ${fails}`);process.exit(fails?1:0);
