// Check that the Music page draws the states it is meant to draw.
//
// Why this exists. The page has more shape than a list: a switch, an apply
// state that appears only with staged changes, rows that read "Staged", an
// inspector that follows the selected row, a progress line while the core
// encodes or builds, and a done line with a dismiss. Every one of those is a
// real state on a real install, and a harness row cannot show what the page
// does with them. This can, before a click does.
//
//     node Tools/check-music-page.mjs
//
// Set MUSIC_JS to check a different file, which is how the checker itself is
// checked: point it at a copy with a guard removed and it must fail.

import { readFileSync } from "fs";

const file = process.env.MUSIC_JS || "crates/reborn-ui/ui/music.js";
const src = readFileSync(file, "utf8");

// A DOM that records structure, so assertions can read it back.
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

// ---------------------------------------------------------------------------
// A real-looking status: one custom song applied, one reset staged.
// ---------------------------------------------------------------------------

function slot(n, extra = {}) {
  return { slot: n, title: "", artist: "", album: "", duration_ms: null, source: "original", source_name: null, pending: null, ...extra };
}
const slots = Array.from({ length: 21 }, (_, i) => slot(i + 1));
slots[20] = slot(21, { title: "Un-thinkable", artist: "Alicia Keys", album: "The Element of Freedom", duration_ms: 286_000, source: "your_file", source_name: "a1b2.mp3" });
slots[2] = slot(3, { pending: { kind: "original", title: "", artist: "", album: "" } });
const status = {
  phase: "idle",
  progress: null,
  enabled: true,
  base: { kind: "original", name: null },
  slots,
  pending_count: 1,
  note: "",
  image_ok: true,
  image_note: "",
  limit_seconds: 380,
  formats: ["mp3", "m4a", "flac", "ogg", "wav", "aiff"],
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
  "\nexport { draw, select, __set, __state };" +
  "\nfunction __set(s) { if ('status' in s) status = s.status; if ('busy' in s) busy = s.busy; if ('problem' in s) problem = s.problem; if ('notice' in s) notice = s.notice; }" +
  "\nfunction __state() { return { status, busy, problem, notice, selected, picked, fields }; }";
const { draw, select, __set, __state } = await import("data:text/javascript;base64," + Buffer.from(mod).toString("base64"));
await wait();
await wait();
await wait();

const screen = () => byId["screen"];
const rows = () => walk(screen()).filter((n) => n.className.split(" ").includes("music-track"));
const buttons = (label) => walk(screen()).filter((n) => n.tagName === "button" && n.textContent === label);
const inspector = () => walk(screen()).find((n) => n.className.split(" ").includes("music-inspector"));

check("the page read music_status on boot", calls.some(([c]) => c === "music_status"));
check("21 track rows are drawn", rows().length === 21, String(rows().length));
check("slot 1 is selected first and the inspector shows it", __state().selected === 1 && text(inspector()).includes("Slot 1") && text(inspector()).includes("Original song 1"));
check("the custom song shows its title and artist in the list", text(screen()).includes("Un-thinkable") && text(screen()).includes("Alicia Keys"));
check("the staged reset reads as staged", text(screen()).includes("Staged: the original song"));
check("the apply state appears with staged changes", text(screen()).includes("1 change staged") && buttons("Apply changes").length === 1);
check("the switch reads On", text(screen()).includes("Turn off"));

// Selecting the custom song: the inspector follows, offers Reset, fields prefilled.
{
  const row = rows().find((r) => r.attrs["data-slot"] === "21");
  row.listeners.click[0]();
  const s = __state();
  check("clicking a row selects it and fills the fields", s.selected === 21 && s.fields.title === "Un-thinkable" && s.fields.artist === "Alicia Keys");
  check("a custom slot offers Reset to original", walk(inspector()).some((n) => n.attrs["aria-label"] === "Reset slot 21 to the original song"));
  check("an original slot does not", (() => { select(1); return !walk(inspector()).some((n) => (n.attrs["aria-label"] || "").startsWith("Reset slot")); })());
}

// Add a song: first free slot selected, file chosen, details filled, Add stages it.
{
  const add = buttons("Add a song")[0];
  await add.listeners.click[0]();
  await wait(); await wait();
  const s = __state();
  check("Add a song selects the first free slot and inspects the chosen file", s.selected === 1 && s.picked && s.fields.title === "Un-thinkable (I'm Ready)" && calls.some(([c]) => c === "music_inspect"));
  check("the lookup candidate is offered", text(inspector()).includes("(iTunes)"));
  const addButton = buttons("Add to slot 1")[0];
  check("the primary action names the slot", !!addButton);
  await addButton.listeners.click[0]();
  await wait();
  check("adding sends the file, slot and fields", calls.some(([c, a]) => c === "music_stage_file" && a.slot === 1 && a.title === "Un-thinkable (I'm Ready)" && a.path === inspectView.path));
  check("the encoding phase draws progress", text(screen()).includes("Encoding slot 1"));
}

// Apply sends confirm and the page shows the building state it answered with.
{
  __set({ status, busy: false, problem: null, notice: null });
  draw();
  const apply = buttons("Apply changes")[0];
  await apply.listeners.click[0]();
  await wait();
  check("apply sends confirm: true", calls.some(([c, a]) => c === "music_apply" && a.confirm === true));
  check("the building phase draws progress and disables actions", text(screen()).includes("Building your game disc") && buttons("Apply changes").every((b) => b.disabled));
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

// No status at all (the read failed): the failure is on screen.
{
  __set({ status: null, busy: false, problem: "Your soundtrack could not be read: failed on purpose", notice: null });
  draw();
  check("a failed read is said on screen", text(screen()).includes("failed on purpose"));
  __set({ status: { ...status, image_ok: false, image_note: "Import the game first." } });
  draw();
  check("without an image the switch is disabled", buttons("Turn off").every((b) => b.disabled) && text(screen()).includes("Import the game first."));
}

// The switch sends the opposite of the current state.
{
  __set({ status, busy: false, problem: null, notice: null });
  draw();
  await buttons("Turn off")[0].listeners.click[0]();
  await wait();
  check("turning off sends on: false", calls.some(([c, a]) => c === "music_set_enabled" && a.on === false));
}

console.log(fails ? `\n${fails} check(s) FAILED` : "\nall checks passed");
process.exit(fails ? 1 : 0);
