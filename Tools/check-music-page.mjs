// Check that the Music page draws the states it is meant to draw.
//
// Why this exists. The page walks a person through the old editor's three
// steps: choose a file, check the details, choose the slot, then one apply.
// Steps 2 and 3 stay off until a file is chosen; the queue lists what is
// staged; the disc list offers Change and Reset; a rebuild draws progress
// and a done line. Every one of those is a real state on a real install, and
// a harness row cannot show what the page does with them. This can, before a
// click does.
//
//     node Tools/check-music-page.mjs
//
// Set MUSIC_JS to check a different file, which is how the checker itself is
// checked: point it at a copy with a guard removed and it must fail.

import { readFileSync } from "fs";

const file = process.env.MUSIC_JS || "crates/reborn-ui/ui/music.js";
const src = readFileSync(file, "utf8");

function fakeEl(tag) {
  return {
    tagName: tag,
    style: {},
    children: [],
    className: "",
    textContent: "",
    value: "",
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
    remove() {},
    close() {},
    showModal() {},
    scrollIntoView() {},
    get classList() {
      const self = this;
      return { add(c) { self.className += " " + c; }, toggle() {} };
    },
  };
}
const byId = {};
global.document = {
  body: fakeEl("body"),
  getElementById: (id) => (byId[id] ||= fakeEl("div#" + id)),
  createElement: (tag) => fakeEl(tag),
  querySelector: () => null,
  querySelectorAll: () => [],
};

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
global.setTimeout = (f) => { return { f }; };
global.clearTimeout = () => {};

function text(node) {
  if (node === null || node === undefined) return "";
  if (typeof node === "string") return node;
  return [node.textContent || "", ...node.children.map(text)].join(" ");
}
function walk(node, out = []) {
  if (!node || typeof node === "string") return out;
  out.push(node);
  for (const c of node.children) walk(c, out);
  return out;
}
const realSetTimeout = (await import("timers")).setTimeout;
const wait = () => new Promise((r) => realSetTimeout(r, 0));

let fails = 0;
function check(what, ok, detail = "") {
  if (!ok) fails++;
  console.log(`${ok ? "ok  " : "FAIL"}  ${what}${ok ? "" : `: ${detail}`}`);
}

// A real-looking status: one custom song applied, one reset staged.
function slot(n, extra = {}) {
  return { slot: n, title: "", artist: "", album: "", duration_ms: null, source: "original", source_name: null, pending: null, ...extra };
}
const slots = Array.from({ length: 21 }, (_, i) => slot(i + 1));
slots[20] = slot(21, { title: "Un-thinkable", artist: "Alicia Keys", album: "The Element of Freedom", duration_ms: 286_000, source: "your_file", source_name: "a1b2.mp3" });
slots[2] = slot(3, { pending: { kind: "original", title: "", artist: "", album: "" } });
const status = {
  phase: "idle", progress: null, enabled: true, base: { kind: "original", name: null }, slots,
  pending_count: 1, note: "", image_ok: true, image_note: "", limit_seconds: 380, formats: ["mp3", "m4a", "flac", "ogg", "wav", "aiff"],
};
const inspectView = {
  path: "C:\\Music\\03. Alicia Keys - Unthinkable (Official Video).mp3",
  title: "Un-thinkable (I'm Ready)", artist: "Alicia Keys", album: "The Element of Freedom",
  details_from: "lookup", duration_ms: 286_000, too_long: false, query: "Alicia Keys Unthinkable",
  candidates: [{ title: "Un-thinkable (I'm Ready)", artist: "Alicia Keys", album: "The Element of Freedom", duration_ms: 286_000, source: "iTunes" }],
  note: "Details found online from the file name. Check them.",
};
answers = {
  music_status: status,
  music_unstage: status,
  music_stage_stock: status,
  music_stage_details: status,
  music_stage_file: { ...status, phase: "encoding", progress: { label: "Encoding slot 1", done: 3, total: 40 } },
  music_apply: { ...status, phase: "building", progress: { label: "Building your game disc", done: 700_000_000, total: 1_459_978_240 }, pending_count: 1 },
  music_dismiss: { ...status, phase: "idle" },
  music_set_enabled: { ...status, enabled: false },
  pick_path: (args) => (args && args.kind === "audio" ? inspectView.path : null),
  music_inspect: inspectView,
  music_lookup: inspectView.candidates,
};

const mod =
  src +
  "\nexport { draw, __set, __state };" +
  "\nfunction __set(s) { if ('status' in s) status = s.status; if ('busy' in s) busy = s.busy; if ('problem' in s) problem = s.problem; if ('notice' in s) notice = s.notice; }" +
  "\nfunction __state() { return { status, busy, problem, notice, picked, editing, fields, targetSlot }; }";
const { draw, __set, __state } = await import("data:text/javascript;base64," + Buffer.from(mod).toString("base64"));
await wait();
await wait();
await wait();

const screen = () => byId["screen"];
const buttons = (label) => walk(screen()).filter((n) => n.tagName === "button" && n.textContent === label);
const byLabel = (label) => walk(screen()).find((n) => n.attrs["aria-label"] === label);
const section = (id) => walk(screen()).find((n) => n.attrs["id"] === id);
const rows = () => walk(screen()).filter((n) => n.className.split(" ").includes("mz-track"));

check("the page read music_status on boot", calls.some(([c]) => c === "music_status"));
check("the disc's state is one sentence at the top", text(screen()).includes("Your disc plays 1 custom song"));
check("step 1 offers the file button and steps 2 and 3 are off", buttons("Choose a file").length === 1 && section("step-details").attrs["aria-disabled"] === "true" && section("step-slot").attrs["aria-disabled"] === "true");
check("the queue lists the staged reset with a Remove", text(section("queue")).includes("Slot 3: the original song") && byLabel("Remove the change for slot 3"));
check("the queue offers one Apply", buttons("Apply to the game").length === 1);
check("the disc list has 21 rows with the custom song", rows().length === 21 && text(section("disc")).includes("Un-thinkable by Alicia Keys"));
check("a custom song offers Reset, an original does not", !!byLabel("Reset slot 21 to the original song") && !byLabel("Reset slot 1 to the original song"));

// Choose a file: steps 2 and 3 come on, details are filled, the first free slot is picked.
{
  await buttons("Choose a file")[0].listeners.click[0]();
  await wait(); await wait();
  const s = __state();
  check("choosing a file inspects it and fills the details", calls.some(([c]) => c === "music_inspect") && s.fields.title === "Un-thinkable (I'm Ready)" && s.fields.artist === "Alicia Keys");
  check("steps 2 and 3 are on after a file is chosen", section("step-details").attrs["aria-disabled"] === "false" && section("step-slot").attrs["aria-disabled"] === "false");
  check("the first untouched slot is picked (slot 1)", s.targetSlot === 1 && text(section("step-slot")).includes("Add to the queue: slot 1"));
  check("the lookup candidate is offered", text(section("step-details")).includes("(iTunes)"));
  const add = buttons("Add to the queue: slot 1")[0];
  await add.listeners.click[0]();
  await wait();
  check("adding sends the file, slot and fields", calls.some(([c, a]) => c === "music_stage_file" && a.slot === 1 && a.title === "Un-thinkable (I'm Ready)" && a.path === inspectView.path));
  check("the encoding phase draws progress", text(screen()).includes("Encoding slot 1"));
  check("the flow resets after adding", __state().picked === null && buttons("Choose a file").length === 1);
}

// Change on a disc row: steps 2 and 3 with the slot fixed, Save sends details only.
{
  __set({ status, busy: false, problem: null, notice: null });
  draw();
  byLabel("Change the details of slot 21").listeners.click[0]();
  const s = __state();
  check("Change opens the details for that slot with its text", s.editing === 21 && s.fields.title === "Un-thinkable" && text(section("step-choose")).includes("Changing the details of slot 21"));
  await buttons("Add the new details to the queue")[0].listeners.click[0]();
  await wait();
  check("saving details sends the slot and fields, no file", calls.some(([c, a]) => c === "music_stage_details" && a.slot === 21 && a.title === "Un-thinkable"));
}

// Apply sends confirm and the page shows the building state it answered with.
{
  __set({ status, busy: false, problem: null, notice: null });
  draw();
  await buttons("Apply to the game")[0].listeners.click[0]();
  await wait();
  check("apply sends confirm: true", calls.some(([c, a]) => c === "music_apply" && a.confirm === true));
  check("the building phase draws progress and disables apply", text(screen()).includes("Building your game disc") && buttons("Apply to the game").every((b) => b.disabled));
}

// A done phase shows the note with a dismiss.
{
  __set({ status: { ...status, phase: "done", note: "Your soundtrack is on the disc: 1 custom song." }, busy: false });
  draw();
  check("a finished rebuild says what it did", text(screen()).includes("Your soundtrack is on the disc") && buttons("Got it").length === 1);
}

// Every field null: the page still draws.
{
  const nulls = Object.fromEntries(Object.keys(status).map((k) => [k, null]));
  __set({ status: nulls, busy: false, problem: null, notice: null });
  try {
    draw();
    check("the page survives a status with every field null", text(screen()).trim().length > 0);
  } catch (e) {
    check("the page survives a status with every field null", false, e.message);
  }
}

// No status at all: the failure is on screen. No image: the file button is off.
{
  __set({ status: null, busy: false, problem: "Your soundtrack could not be read: failed on purpose", notice: null });
  draw();
  check("a failed read is said on screen", text(screen()).includes("failed on purpose"));
  __set({ status: { ...status, image_ok: false, image_note: "Import the game first." } });
  draw();
  check("without an image the file button is disabled and the reason shown", buttons("Choose a file").every((b) => b.disabled) && text(screen()).includes("Import the game first."));
}

// The switch sends the opposite of the current state.
{
  __set({ status, busy: false, problem: null, notice: null });
  draw();
  await buttons("Turn the custom soundtrack off")[0].listeners.click[0]();
  await wait();
  check("turning off sends on: false", calls.some(([c, a]) => c === "music_set_enabled" && a.on === false));
}

console.log(fails ? `\n${fails} check(s) FAILED` : "\nall checks passed");
process.exit(fails ? 1 : 0);
