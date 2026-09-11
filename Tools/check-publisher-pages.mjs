// Check that every Publisher page survives every answer the core can give,
// and that the one-button job on each page holds its contract.
//
// Eight pages (Home, Content, Launcher, Roster, Roll back, Build an ISO,
// Full installer set, Keys) each read several commands and draw from
// whatever came back. Each page is loaded three times, with a fake core
// that answers everything, answers with every field null, and rejects
// every call, and each time the page has to draw without throwing, with
// nothing on screen blank without an explanation.
//
// Then the job contract (`job.js`), driven on the Roster page with a tiny
// op_status state machine: the primary button is there before anything is
// clicked; pressing it calls the plan command and never the run command;
// the card appears with "Yes, publish" only once the check settled; a
// refusal is one sentence with a Copy button; another page's busy
// operation disables the button and names that page; another page's
// finished operation is ignored.
//
// Shape follows Tools/check-shell-pages.mjs: a fake DOM that records
// structure, a fake `window.__TAURI__.core.invoke`, FULL/NULLS/reject.
//
//     node Tools/check-publisher-pages.mjs

import { readFileSync } from "fs";

// ---- a DOM that records structure ------------------------------------------
function fakeEl(tag) {
  return {
    tagName: tag, children: [], className: "", textContent: "", value: "", checked: false, disabled: false, selected: false, hidden: false, attrs: {}, listeners: {}, style: {},
    get firstChild() { return this.children[0]; },
    append(...c) { this.children.push(...c); },
    replaceChildren(...c) { this.children = c; },
    remove() {},
    select() {},
    addEventListener(k, f) { (this.listeners[k] ||= []).push(f); },
    setAttribute(k, v) { this.attrs[k] = v; },
    querySelectorAll() { return []; },
    get classList() {
      const self = this;
      return { add(c) { self.className = (self.className + " " + c).trim(); }, remove() {}, contains(c) { return self.className.split(" ").includes(c); } };
    },
  };
}
let byId = {};
global.document = {
  body: { dataset: {}, append() {} },
  dispatchEvent() {},
  getElementById: (id) => (byId[id] ||= fakeEl("div#" + id)),
  createElement: (tag) => fakeEl(tag),
  createTextNode: (t) => ({ tagName: "#text", children: [], textContent: t ?? "" }),
  addEventListener() {},
  querySelectorAll: () => [],
  execCommand: () => true,
};
global.location = { pathname: "/index.html" };
global.localStorage = { getItem: () => null, setItem() {}, removeItem() {} };
// Node's own `navigator` is a getter-only global; define over it.
Object.defineProperty(global, "navigator", { value: { clipboard: { writeText: async () => {} } }, configurable: true });
const tick = () => new Promise((r) => setTimeout(r, 0));
async function settle(n) {
  for (let i = 0; i < n; i++) await tick();
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function text(node) {
  if (node === null || node === undefined) return "";
  if (typeof node === "string") return node;
  return [node.textContent || "", ...node.children.map(text)].join(" ");
}

/// Every element under `node` (depth first), including `node`.
function all(node, out = []) {
  if (!node || typeof node !== "object") return out;
  out.push(node);
  for (const c of node.children || []) all(c, out);
  return out;
}

function buttons(node) {
  return all(node).filter((n) => n.tagName === "button");
}

function buttonNamed(node, re) {
  return buttons(node).find((b) => re.test(b.textContent || "")) || null;
}

async function click(button) {
  for (const f of button.listeners.click || []) await f({ currentTarget: button });
}

// ---- answers shaped like the real commands ---------------------------------
const FULL = {
  publisher_status: {
    problems: [],
    local: {
      version: "3.0.1",
      sequence: 9,
      pins: [{ component: "roster", version: "2026.09.06" }, { component: "textures", version: "1.2.0" }],
      components_on_disk: [{ component: "roster", version: "2026.09.06", sequence: 3 }],
      unpinned: [],
    },
    hosted: {
      version: "3.0.1",
      sequence: 9,
      pins: [{ component: "roster", version: "2026.09.06" }, { component: "textures", version: "1.2.0" }, { component: "game-patch", version: "NSRV3" }],
      verified: true,
    },
    launcher: { version: "3.0.0", bytes: 52428800, sha256: "ab12cd34ef", file_name: "NSR Launcher Update 3.0.0.exe" },
    pending: [],
  },
  publisher_environment: {
    items: [
      { key: "publisher_key", label: "Signing key", kind: "file", found: true, path: "C:\\keys\\publisher.key", note: null },
      { key: "builder", label: "Reborn Builder", kind: "file", found: false, path: "C:\\tools\\reborn-builder-cli.exe", note: null },
      { key: "site_token", label: "Site token", kind: "vault", found: true, path: null, note: null },
    ],
    problems: [],
  },
  publisher_recent: {
    records: [
      {
        schema: 1, at: "2026-09-09T12:00:00+00:00", operation: "roster", dry_run: false, inputs: {},
        sequence: 3, hashes: [], uploaded: [], verified: null, outcome: "done", summary: "Published roster 2026.09.06.",
      },
    ],
    error: null,
    problems: [],
  },
  publisher_settings: {
    origin: "C:\\Reborn Origin",
    publisher_key: "C:\\keys\\publisher.key",
    builder: "C:\\tools\\reborn-builder-cli.exe",
    xdelta: "C:\\tools\\xdelta3.exe",
    bucket_endpoint: "https://abc123.r2.cloudflarestorage.com",
    bucket_name: "reborn-files",
    public_base: "https://files.nflstreetreborn.com/",
    launcher_manifest: "https://files.nflstreetreborn.com/launcher/latest.json",
    remembered: {},
  },
  publisher_set_setting: {},
  op_status: { phase: "idle", operation: "", lines: [], summary: "", plan: null, dry_run: false, progress: null },
  keys_state: {
    trusted: [{ role: "everyday", public_hex: "aa11" }, { role: "rotation", public_hex: "bb22" }],
    publisher_key: { configured: true, path: "C:\\keys\\publisher.key", exists: true, public_hex: "aa11", trust: "everyday" },
    rotation_key: { path: "C:\\keys\\rotation.key", exists: true },
    same_folder_warning: false,
  },
  vault_state: {
    site_token: { stored: true },
    r2_access_key: { stored: true },
    r2_secret: { stored: true },
    offer_import: false,
  },
  history_list: {
    records: [
      {
        schema: 1, at: "2026-09-09T12:00:00+00:00", operation: "roster", dry_run: false, inputs: { kind: "roster-run" },
        sequence: 3, hashes: [["roster", "abcd1234"]], uploaded: ["components/roster-2026.09.06.json"],
        verified: "confirmed", outcome: "done", summary: "Published roster 2026.09.06.",
      },
    ],
    shown: 1,
    total: 1,
    skipped: 0,
  },
  history_versions: {
    component: "roster",
    current_version: "2026.09.06",
    versions: [
      { version: "2026.09.06", sequence: 3, files: 4, bytes: 2048, current: true },
      { version: "2026.08.28", sequence: 2, files: 4, bytes: 2000, current: false },
    ],
    next_sequence_if_published_new: 4,
  },
};
const NULLS = Object.fromEntries(
  Object.entries(FULL).map(([k, v]) => [k, Array.isArray(v) ? null : Object.fromEntries(Object.keys(v).map((f) => [f, null]))])
);

let fails = 0;
let current = "(not loading any page)";
process.on("unhandledRejection", (e) => {
  fails++;
  const msg = e && e.message ? e.message : String(e);
  console.log(`FAIL  ${current}: unhandled rejection: ${msg.slice(0, 200)}`);
});

function check(what, ok, detail = "") {
  if (!ok) fails++;
  console.log(`${ok ? "ok  " : "FAIL"}  ${what}${ok ? "" : `: ${detail}`}`);
}

// The pages import ./shell.js and (every page but Keys) ./job.js, neither of
// which a data: URL can resolve, so both shared modules are inlined.
const SHELL = readFileSync("crates/reborn-publisher/ui/shell.js", "utf8").replaceAll("export ", "");
const IMPORT_SHELL = /^import .*from ['"]\.\/shell\.js['"];\r?\n/m;
const IMPORT_JOB = /^import \{ job \} from ['"]\.\/job\.js['"];\r?\n/m;
const JOB = readFileSync("crates/reborn-publisher/ui/job.js", "utf8").replace(IMPORT_SHELL, "").replaceAll("export ", "");

async function loadPage(file, answers, mode, hooks = {}) {
  const calls = [];
  byId = {};
  global.window = {
    addEventListener() {},
    __TAURI__: {
      core: {
        invoke: async (cmd, args = {}) => {
          calls.push({ cmd, args });
          if (hooks.invoke) {
            const hooked = await hooks.invoke(cmd, args);
            if (hooked !== undefined) return hooked;
          }
          if (mode === "reject") throw new Error(`${cmd} failed on purpose`);
          if (!(cmd in answers)) throw new Error(`no fake answer for ${cmd}`);
          return answers[cmd];
        },
      },
    },
  };
  byId["screen"] = fakeEl("div#screen");
  const src = readFileSync(file, "utf8").replace(IMPORT_SHELL, "").replace(IMPORT_JOB, "");
  const mod = SHELL + "\n" + JOB + "\n" + src + `\n// ${file} ${mode} ${Math.random()}`;
  await import("data:text/javascript;base64," + Buffer.from(mod).toString("base64"));
  await settle(8);
  return { calls, screen: byId["screen"] };
}

const PAGES = [
  ["crates/reborn-publisher/ui/status.js", "Home"],
  ["crates/reborn-publisher/ui/roster.js", "Roster"],
  ["crates/reborn-publisher/ui/launcher.js", "Launcher update"],
  ["crates/reborn-publisher/ui/game.js", "Build an ISO"],
  ["crates/reborn-publisher/ui/content.js", "Textures, runtime, mods"],
  ["crates/reborn-publisher/ui/set.js", "Full installer set"],
  ["crates/reborn-publisher/ui/history.js", "Roll back"],
  ["crates/reborn-publisher/ui/keys.js", "Keys"],
];

for (const [file, name] of PAGES) {
  for (const mode of ["full", "nulls", "reject"]) {
    const answers = mode === "nulls" ? NULLS : FULL;
    const how = mode === "full" ? "answers everything" : mode === "nulls" ? "answers with every field null" : "rejects every call";
    current = `${name} (${how})`;
    try {
      const { calls, screen } = await loadPage(file, answers, mode);
      const drawn = text(screen).trim().length > 0;
      check(`${name} draws when the core ${how}`, drawn, `nothing drawn; calls: ${calls.map((c) => c.cmd).join(",")}`);
      if (mode === "reject") {
        check(`${name} says what failed`, /failed on purpose/i.test(text(screen)), text(screen).slice(0, 300));
      }
      if (mode !== "reject" && name !== "Keys") {
        // Nothing on a page's face may say "Dry run" or "Plan" or "Verify"
        // as a control any more: the job is one button.
        const labels = buttons(screen).map((b) => b.textContent);
        check(`${name} has no Plan/Run/Verify/Dry run controls`, !labels.some((l) => /^(Plan|Run|Verify)$/.test(l)) && !/Dry run/.test(text(screen)), labels.join(" | "));
      }
    } catch (e) {
      check(`${name} survives mode ${mode}`, false, e.message.slice(0, 300));
    }
    await settle(1);
  }
}
current = "(between page checks)";

// ---- a few pointed checks ---------------------------------------------------

{
  const { screen } = await loadPage("crates/reborn-publisher/ui/status.js", FULL, "full");
  const t = text(screen);
  check("Home names what players get right now", /What players get right now/.test(t) && /Launcher/.test(t) && /3\.0\.0/.test(t), t.slice(0, 300));
  check("Home keeps the launcher version and the content release apart", /content release 9/i.test(t) && /Launcher\s+3\.0\.0/.test(t.replace(/\s+/g, " ")), t.slice(0, 400));
  check("Home lists only what is missing on this computer", /Reborn Builder: missing/.test(t) && !/Signing key: missing/.test(t), t.slice(0, 400));
  check("Home has an Upload now button", !!buttonNamed(screen, /^Upload now$/), buttons(screen).map((b) => b.textContent).join(" | "));
  check("Home says nothing is pending", /The server has everything this computer published/.test(t), t.slice(0, 400));
}
{
  const { screen } = await loadPage("crates/reborn-publisher/ui/status.js", NULLS, "nulls");
  const t = text(screen);
  check("Home explains an unreadable release", /No release could be read from the server/i.test(t), t.slice(0, 300));
}
{
  const { screen } = await loadPage("crates/reborn-publisher/ui/roster.js", FULL, "full");
  const b = buttonNamed(screen, /^Publish roster \d{4}\.\d{2}\.\d{2}$/);
  check("Roster's one button says what it will do", !!b && !b.disabled, buttons(screen).map((x) => x.textContent).join(" | "));
}
{
  const { screen } = await loadPage("crates/reborn-publisher/ui/content.js", FULL, "full");
  const t = text(screen);
  check("Content says what players have and what is next", /Players have roster 2026\.09\.06 now/.test(t) === false && /Players have textures|Next:/.test(t), t.slice(0, 300));
}
{
  const { screen } = await loadPage("crates/reborn-publisher/ui/launcher.js", FULL, "full");
  const b = buttonNamed(screen, /^Build and publish launcher 3\.0\.1$/);
  check("Launcher proposes the next version on its button", !!b, buttons(screen).map((x) => x.textContent).join(" | "));
}
{
  const { screen } = await loadPage("crates/reborn-publisher/ui/history.js", FULL, "full");
  const b = buttonNamed(screen, /^Roll roster back to 2026\.08\.28$/);
  check("Roll back's button names the thing and the version", !!b, buttons(screen).map((x) => x.textContent).join(" | "));
}
{
  const { screen } = await loadPage("crates/reborn-publisher/ui/keys.js", NULLS, "nulls");
  const t = text(screen);
  check("Keys explains an unreadable signing-key state", /No signing key is configured yet|Generate one below/i.test(t), t.slice(0, 300));
}

// ---- the job contract, on the Roster page -----------------------------------

async function rosterWith(op, invoke) {
  let state = op;
  const hooks = {
    invoke: async (cmd, args) => {
      if (cmd === "op_status") return JSON.parse(JSON.stringify(state));
      if (cmd === "op_cancel") return null;
      if (invoke) {
        const r = await invoke(cmd, args, (s) => { state = s; });
        if (r !== undefined) return r;
      }
      return undefined;
    },
  };
  const loaded = await loadPage("crates/reborn-publisher/ui/roster.js", FULL, "full", hooks);
  return { ...loaded, setState: (s) => { state = s; } };
}

const PLANNED = {
  phase: "planned", operation: "roster", lines: ["Roster        C:/x.rebornroster", "Read          569 players"],
  summary: "Roster 2026.09.10: 569 players, 32 teams.", dry_run: true, progress: null,
  plan: { summary: "x", players: 569, teams: 32, portraits: 40, base_label: "2026.08.28", file_bytes: 2048, version: "2026.09.10", current_pins: [], proposed_pins: [], release_bootstrap_needed: false, release_will_be_signed: true },
};

{
  current = "job: check then card";
  const { screen, calls } = await rosterWith(FULL.op_status, async (cmd, args, set) => {
    if (cmd === "roster_plan") { set(PLANNED); return { accepted: true }; }
    if (cmd === "roster_run") { set({ ...PLANNED, phase: "done", summary: "Published roster 2026.09.10.", dry_run: false }); return { accepted: true }; }
    if (cmd === "roster_verify") { set({ ...PLANNED, phase: "done", summary: "The server pins roster 2026.09.10.", dry_run: false }); return { accepted: true }; }
    return undefined;
  });
  const primary = buttonNamed(screen, /^Publish roster/);
  check("job: no card before the button is pressed", !/Yes, publish/.test(text(screen)), text(screen).slice(0, 200));
  await click(primary);
  await sleep(450);
  await settle(4);
  const t = text(screen);
  check("job: pressing the button called the plan command and not the run command", calls.some((c) => c.cmd === "roster_plan") && !calls.some((c) => c.cmd === "roster_run"), calls.map((c) => c.cmd).join(","));
  check("job: the card says it in plain words", /569 players, 32 teams, 40 portraits/.test(t) && /2 KB|2\.00 KB/.test(t), t.slice(0, 400));
  const yes = buttonNamed(screen, /^Yes, publish$/);
  check("job: the card offers Yes, publish and Cancel", !!yes && !!buttonNamed(screen, /^Cancel$/), buttons(screen).map((b) => b.textContent).join(" | "));
  await click(yes);
  await sleep(450);
  await settle(4);
  await sleep(450);
  await settle(4);
  const runCall = calls.find((c) => c.cmd === "roster_run");
  check("job: Yes runs for real (dry run off)", !!runCall && runCall.args.dry_run === false, JSON.stringify(runCall && runCall.args));
  check("job: verify ran on its own after the run", calls.some((c) => c.cmd === "roster_verify"), calls.map((c) => c.cmd).join(","));
  const done = text(screen);
  check("job: the done card says what players now get and when it was verified", /Done\. Players now get roster 2026\.09\.10/.test(done) && /Verified from the server at/.test(done), done.slice(0, 500));
}

{
  current = "job: refusal";
  const REFUSED = { phase: "refused", operation: "roster", lines: ["Roster        C:/x"], summary: "A User folder was found under this tree.", plan: null, dry_run: true, progress: null };
  const { screen } = await rosterWith(FULL.op_status, async (cmd, args, set) => {
    if (cmd === "roster_plan") { set(REFUSED); return { accepted: true }; }
    return undefined;
  });
  await click(buttonNamed(screen, /^Publish roster/));
  await sleep(450);
  await settle(4);
  const t = text(screen);
  check("job: a refusal is one sentence with a Copy button", /A User folder was found under this tree/.test(t) && !!buttonNamed(screen, /^Copy$/), t.slice(0, 300));
  check("job: the raw log sits behind Details", all(screen).some((n) => n.tagName === "details") && /Roster        C:\/x/.test(t), t.slice(0, 300));
}

{
  current = "job: another page busy";
  const BUSY = { phase: "running", operation: "content", lines: ["Hashing..."], summary: "", plan: null, dry_run: false, progress: null };
  const { screen } = await rosterWith(BUSY);
  const primary = buttonNamed(screen, /^Publish roster/);
  const t = text(screen);
  check("job: another page's running operation disables the button and names that page", primary && primary.disabled && /Textures, runtime, mods is publishing\. Open Textures, runtime, mods to watch\./.test(t), t.slice(0, 300));
  check("job: another page's log is not shown here", !/Hashing\.\.\./.test(t), t.slice(0, 300));
}

{
  current = "job: another page finished";
  const FOREIGN = { phase: "planned", operation: "content", lines: ["Listed 16,796 object(s)"], summary: "2 object(s) pending upload", plan: { summary: "2 object(s) pending upload" }, dry_run: true, progress: null };
  const { screen } = await rosterWith(FOREIGN);
  const primary = buttonNamed(screen, /^Publish roster/);
  const t = text(screen);
  check("job: another page's finished operation is ignored", primary && !primary.disabled && !/pending upload/.test(t) && !/Yes, publish/.test(t), t.slice(0, 300));
}

{
  current = "job: progress bar";
  const { screen, setState } = await rosterWith(FULL.op_status, async (cmd, args, set) => {
    if (cmd === "roster_plan") { set(PLANNED); return { accepted: true }; }
    if (cmd === "roster_run") { set({ ...PLANNED, phase: "running", dry_run: false, progress: { done: 1536, total: 2048, label: "Uploading" } }); return { accepted: true }; }
    return undefined;
  });
  await click(buttonNamed(screen, /^Publish roster/));
  await sleep(450);
  await settle(4);
  // Not awaited: the confirm click resolves only once the run settles,
  // which this script does by hand after looking at the bar.
  const confirmed = click(buttonNamed(screen, /^Yes, publish$/));
  await sleep(450);
  await settle(4);
  const bar = all(screen).find((n) => n.className === "job-bar-fill");
  const t = text(screen);
  check("job: bytes moving draw a bar with the numbers", bar && bar.style.width === "75%" && /1\.50 KB of 2\.00 KB \(75%\)/.test(t), `${bar && bar.style.width} ${t.slice(0, 200)}`);
  setState({ ...PLANNED, phase: "done", dry_run: false, summary: "Published." });
  await confirmed;
  await settle(4);
}

console.log(fails === 0 ? "\nPASS: every Publisher page survives every answer, and the job holds its contract" : `\nFAIL: ${fails}`);
process.exit(fails ? 1 : 0);
