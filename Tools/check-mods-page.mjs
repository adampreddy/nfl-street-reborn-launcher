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
        return a;
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

const library = {
  packages_root: "C:\\data\\mods\\packages",
  packages: [...footballs, ...controls, standalone],
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
  mods_toggle: syncDone,
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

// ---- boot read ----------------------------------------------------------------
check("boot reads the library, the options and the current profile",
  ["mod_packages", "controls_options", "controls_current"].every((c) => calls.some(([k]) => k === c)),
  calls.map((c) => c[0]).join(","));

// ---- the shape of the real machine -------------------------------------------
{
  const picks = sections("Pick one");
  check("seven footballs draw as ONE pick-one group, and the controls pair does not", picks.length === 1, `${picks.length} groups`);
  const radios = picks.length ? inputs(picks[0]).filter((i) => i.attrs.type === "radio") : [];
  check("the group has None plus seven", radios.length === 8, `${radios.length} radios`);
  check("exactly one is checked, the active one", radios.filter((r) => r.checked).length === 1);
  check("None is not checked while purple is on", radios.length && !radios[0].checked);
  check("the group says WHY it is exclusive", text(picks[0]).includes(BALL));

  const art = sections("Button art");
  check("button art is its own section", art.length === 1);
  const artRadios = art.length ? inputs(art[0]) : [];
  check("three profiles, Xbox checked", artRadios.length === 3 && artRadios[0].checked && !artRadios[1].checked);
  check("it says where the current one came from", text(art[0]).includes("from settings"));
  check("the suggestion is marked, and marked as a suggestion", text(art[0]).includes("your pad suggests"));

  const other = sections("Other mods");
  check("a standalone package is its own switch", other.length === 1 && inputs(other[0]).some((i) => i.attrs.type === "checkbox"));

  const stale = sections("Listed as on, not installed");
  check("an activation entry with no package is drawn, not dropped", stale.length === 1 && text(stale[0]).includes("jsgme.ghost-football"));

  const bad = sections("Could not be read");
  check("an unreadable package is named", bad.length === 1 && text(bad[0]).includes("not a readable manifest"));

  check("nothing is synced until something is done", text(sections("What the game loads")[0]).includes("Not checked yet"));
}

// ---- clicks do what the rule says -------------------------------------------
{
  calls.length = 0;
  const pick = sections("Pick one")[0];
  const radios = inputs(pick);
  radios[0].listeners.change[0]();   // None
  await tick(); await tick(); await tick();
  const toggled = calls.find(([k]) => k === "mods_toggle");
  check("choosing None turns the active football OFF",
    toggled && toggled[1].id === "jsgme.purple-football" && toggled[1].on === false,
    JSON.stringify(toggled));
  check("the sync that came back is what the page shows", text(sections("What the game loads")[0]).includes("8059"));
  check("a moved-aside folder is named, with nothing deleted", text(sections("What the game loads")[0]).includes("nothing was deleted"));
  check("a listed-but-missing package is shown in the sync facts", text(sections("What the game loads")[0]).includes("jsgme.ghost-football"));

  calls.length = 0;
  const again = inputs(sections("Pick one")[0]);
  const purple = again.find((r, i) => i > 0 && r.checked);
  purple.listeners.change[0]();     // the one already on
  await tick(); await tick();
  check("choosing the football already on sends nothing", !calls.some(([k]) => k === "mods_toggle"));

  const aqua = again[1];
  aqua.listeners.change[0]();
  await tick(); await tick(); await tick();
  const on = calls.find(([k]) => k === "mods_toggle");
  check("choosing another football turns it ON, and only names that one",
    on && on[1].id === "jsgme.aqua-football" && on[1].on === true, JSON.stringify(on));

  calls.length = 0;
  const art = inputs(sections("Button art")[0]);
  art[1].listeners.change[0]();     // PlayStation
  await tick(); await tick(); await tick();
  check("picking button art calls controls_set", calls.some(([k, a]) => k === "controls_set" && a.profile === "playstation"));
  check("a sync the game is holding the lock against is a notice, not a failure",
    __state().notice && __state().notice.includes("next launch") && !__state().problem, JSON.stringify(__state().notice));
}

// ---- busy: nothing is clickable while the tree is being composed ------------
{
  __set({ busy: true, problem: null, notice: null });
  draw();
  const all = walk(screen()).filter((n) => n.tagName === "input" || n.tagName === "button");
  check("every control is disabled while busy", all.length > 0 && all.every((n) => n.disabled === true), `${all.filter((n) => !n.disabled).length} enabled`);
  __set({ busy: false });
}

// ---- states the page has to survive -----------------------------------------
function survives(what, state) {
  __set(state);
  try {
    draw();
    check(`survives ${what}`, true);
  } catch (e) {
    check(`survives ${what}`, false, e.message);
  }
}
survives("nothing read at all", { library: null, options: null, current: null, lastSync: null, problem: null, notice: null });
survives("an empty library", {
  library: { packages_root: "x", packages: [], unreadable: [], activation: [], exclusive: [], managed_by_controls: [] },
  options: [], current: { profile: null, label: null, source: "none" },
});
check("an empty library says so, and says where", text(screen()).includes("No packages are installed under x"));
survives("every field null", {
  library: { packages_root: null, packages: null, unreadable: null, activation: null, exclusive: null, managed_by_controls: null },
  options: { error: "scan failed" }, current: null, lastSync: null,
});
check("a failed profile read is said where the profiles would be", text(sections("Button art")[0]).includes("scan failed"));
survives("a refused sync", { library, options, current, lastSync: { synced: false, refusal: "two write one file", note: "Mods were not applied: two write one file" } });
check("a refusal is drawn as a refusal", walk(screen()).some((n) => n.className === "refusal" && n.textContent.includes("not applied")));
survives("a group whose member could not be read", {
  library: { ...library, packages: library.packages.filter((p) => p.id !== "jsgme.camo-football") },
  lastSync: null,
});
check("the unreadable member is named inside its group", text(sections("Pick one")[0]).includes("jsgme.camo-football is in this group but could not be read"));

console.log(fails === 0 ? "\nPASS: the mods page draws the rules" : `\nFAIL: ${fails}`);
process.exit(fails ? 1 : 0);
