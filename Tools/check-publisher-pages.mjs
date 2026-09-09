// Check that every Publisher page survives every answer the core can give.
//
// Seven pages (Status, Roster, Launcher, Game, Content, History, Keys) each
// read several commands and draw from whatever came back. The failure this
// guards against is the one the launcher's own first-run screens had before
// its own checker existed: a page that works when every read succeeds and
// throws when one field is absent. So each page is loaded three times, with
// a fake core that answers everything, answers with every field null, and
// rejects every call, and each time the page has to draw without throwing,
// with nothing on screen blank without an explanation.
//
// Shape follows Tools/check-shell-pages.mjs exactly: a fake DOM that records
// structure, a fake `window.__TAURI__.core.invoke`, and the same
// FULL/NULLS/reject fixture pattern. Two differences a Publisher page needs
// that a shell page does not:
//
//   - Every Publisher page (except Keys) also imports `./panel.js`, the one
//     shared operation panel. `panel.js` itself imports `./shell.js`. Both
//     shared modules are inlined the same way `check-shell-pages.mjs` inlines
//     `shell.js` alone: their own `import`/`export` lines are stripped so a
//     data: URL (which cannot resolve a relative import) never has to.
//
//   - A page that calls an async function without awaiting it (every page
//     here does this at least once, for its own initial load) turns a thrown
//     error inside that function into an unhandled promise rejection, not a
//     synchronous throw `loadPage`'s own try/catch would see. Node's default
//     is to crash the whole process on an unhandled rejection, which would
//     take down every page's check after the first broken one; a
//     `process.on("unhandledRejection", ...)` below catches that instead and
//     charges it to whichever page/mode was loading when it fired.
//
//     node Tools/check-publisher-pages.mjs

import { readFileSync } from "fs";

// ---- a DOM that records structure ------------------------------------------
function fakeEl(tag) {
  return {
    // A real <input>/<select> defaults `.value` to "" (never undefined) the
    // moment it exists, with no attribute needed to make that true; several
    // pages read `.value.trim()` on a box the page itself never gave an
    // initial value to, trusting that real-DOM default. Without matching it
    // here, this harness would fail a page for a bug only the fake DOM has.
    tagName: tag, children: [], className: "", textContent: "", value: "", checked: false, disabled: false, selected: false, hidden: false, attrs: {}, listeners: {},
    append(...c) { this.children.push(...c); },
    replaceChildren(...c) { this.children = c; },
    addEventListener(k, f) { (this.listeners[k] ||= []).push(f); },
    setAttribute(k, v) { this.attrs[k] = v; },
    querySelectorAll() { return []; },
  };
}
let byId = {};
global.document = {
  body: { dataset: {} },
  dispatchEvent() {},
  getElementById: (id) => (byId[id] ||= fakeEl("div#" + id)),
  createElement: (tag) => fakeEl(tag),
  createTextNode: (t) => ({ tagName: "#text", children: [], textContent: t ?? "" }),
  addEventListener() {},
  querySelectorAll: () => [],
};
global.location = { pathname: "/index.html" };
global.localStorage = { getItem: () => null, setItem() {}, removeItem() {} };
const tick = () => new Promise((r) => setTimeout(r, 0));
async function settle(n) {
  for (let i = 0; i < n; i++) await tick();
}

function text(node) {
  if (node === null || node === undefined) return "";
  if (typeof node === "string") return node;
  return [node.textContent || "", ...node.children.map(text)].join(" ");
}

// ---- answers shaped like the real commands ---------------------------------
//
// Only the commands a page actually calls while drawing its first screen
// (before any button click) need an answer here: every page's Plan/Run/
// Verify commands sit behind the operation panel's buttons, which this
// checker never clicks, since it is only checking the initial draw.
const FULL = {
  publisher_status: {
    problems: [],
    local: {
      version: "3.0.0",
      sequence: 3,
      pins: [{ component: "roster", version: "2026.09.06" }, { component: "launcher", version: "3.0.1" }],
      components_on_disk: [{ component: "roster", version: "2026.09.06", sequence: 3 }],
      unpinned: [],
    },
    hosted: {
      version: "3.0.0",
      sequence: 3,
      pins: [{ component: "roster", version: "2026.09.06" }, { component: "launcher", version: "3.0.1" }],
      verified: true,
    },
    launcher: { version: "3.0.1", bytes: 52428800, sha256: "ab12cd34ef", file_name: "NSR Launcher Update 3.0.1.exe" },
    pending: [],
  },
  publisher_environment: {
    items: [
      { key: "publisher_key", label: "Signing key", kind: "file", found: true, path: "C:\\keys\\publisher.key", note: null },
      { key: "builder", label: "Reborn Builder", kind: "file", found: true, path: "C:\\tools\\reborn-builder-cli.exe", note: null },
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
  op_status: { phase: "idle", operation: "", lines: [], summary: "", plan: null, dry_run: false },
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
// Every field null, and every list answer null too: a list that is absent is
// not an empty list, and a page that assumes it is throws on .length. Same
// one-level transform check-shell-pages.mjs uses: a command's own top-level
// fields all become null, whether that field is a scalar, a list, or an
// object a real answer never actually sends as null itself.
const NULLS = Object.fromEntries(
  Object.entries(FULL).map(([k, v]) => [k, Array.isArray(v) ? null : Object.fromEntries(Object.keys(v).map((f) => [f, null]))])
);

let fails = 0;
// Which page/mode is currently loading, so an unhandled rejection (a page's
// own un-awaited async call throwing) is charged to the right place instead
// of crashing the whole run.
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

// The pages import ./shell.js and (every page but Keys) ./panel.js, neither
// of which a data: URL can resolve, so both shared modules are inlined: their
// own import/export lines become plain declarations.
const SHELL = readFileSync("crates/reborn-publisher/ui/shell.js", "utf8").replaceAll("export ", "");
const IMPORT_SHELL = /^import .*from ['"]\.\/shell\.js['"];\r?\n/m;
const IMPORT_PANEL = /^import \{ panel \} from ['"]\.\/panel\.js['"];\r?\n/m;
// `panel.js` and every page that polls its own richer plan facts
// (roster.js, launcher.js, content.js, history.js) each declare their own
// top-level `POLL_MS`; concatenated into one module, the second declaration
// would be a SyntaxError. `panel.js`'s own copy (and the `BUSY_PHASES` set
// next to it) is renamed on the way in, since it is the one page code never
// refers to by name.
const PANEL = readFileSync("crates/reborn-publisher/ui/panel.js", "utf8")
  .replace(IMPORT_SHELL, "")
  .replaceAll("export ", "")
  .replace(/\bPOLL_MS\b/g, "PANEL_POLL_MS")
  .replace(/\bBUSY_PHASES\b/g, "PANEL_BUSY_PHASES");

async function loadPage(file, answers, mode) {
  const calls = [];
  byId = {};
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
  const src = readFileSync(file, "utf8").replace(IMPORT_SHELL, "").replace(IMPORT_PANEL, "");
  // A fresh module each time: a cache-busting comment on the data: URL.
  const mod = SHELL + "\n" + PANEL + "\n" + src + `\n// ${file} ${mode} ${Math.random()}`;
  await import("data:text/javascript;base64," + Buffer.from(mod).toString("base64"));
  // Publisher pages chain deeper than a shell page's: Promise.all across
  // several commands, then a render, then (on every page but Keys) a
  // panel's own internal poll(); six ticks gives that room without waiting
  // long enough for a real 400ms setInterval to ever fire.
  await settle(6);
  return { calls, screen: byId["screen"] };
}

const PAGES = [
  ["crates/reborn-publisher/ui/status.js", "Status"],
  ["crates/reborn-publisher/ui/roster.js", "Roster"],
  ["crates/reborn-publisher/ui/launcher.js", "Launcher"],
  ["crates/reborn-publisher/ui/game.js", "Game"],
  ["crates/reborn-publisher/ui/content.js", "Content"],
  ["crates/reborn-publisher/ui/history.js", "History"],
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
      check(`${name} draws when the core ${how}`, drawn, `nothing drawn; calls: ${calls.join(",")}`);
      if (mode === "reject") {
        // Every page mounts at least one command call on load (a plain
        // read, or the shared panel's own op_status poll), and every page
        // shows a command's own `error` string rather than swallowing it;
        // the fake core's own rejection message is what proves that.
        check(`${name} says what failed`, /failed on purpose/i.test(text(screen)), text(screen).slice(0, 300));
      }
    } catch (e) {
      check(`${name} survives mode ${mode}`, false, e.message.slice(0, 300));
    }
    // A page's un-awaited setInterval-scheduled work has had a few ticks to
    // misfire against the previous mode's fixture by the time the next mode
    // starts; give any pending microtask one more turn before moving on so
    // a late rejection is charged to this mode, not silently to the next.
    await settle(1);
  }
}
current = "(between page checks)";

// A few pointed checks, the same way check-shell-pages.mjs adds a handful
// beyond the generic loop: not "did it draw something", but "did it draw the
// right explanation for an absence", on the pages and fields most likely to
// go silently blank instead.

// Status: a hosted release that cannot be read says so, by name, rather than
// leaving the Live block's headline blank.
{
  const { screen } = await loadPage("crates/reborn-publisher/ui/status.js", NULLS, "nulls");
  const t = text(screen);
  check("Status explains an unreadable hosted release", /No hosted release could be read/i.test(t), t.slice(0, 300));
  check("Status explains an unreadable environment check", /could not be read|nothing to report/i.test(t), t.slice(0, 300));
}

// Roster: the plan-details block has not gone blank before Plan is ever
// clicked; it says so instead of showing an empty section.
{
  const { screen } = await loadPage("crates/reborn-publisher/ui/roster.js", FULL, "full");
  const t = text(screen);
  check("Roster explains that Plan has not run yet", /Plan has not been run yet/i.test(t), t.slice(0, 300));
}

// Content: the version note explains what it proposes, rather than a blank
// version box with no reasoning shown.
{
  const { screen } = await loadPage("crates/reborn-publisher/ui/content.js", FULL, "full");
  const t = text(screen);
  check("Content explains the proposed version", /Proposed next version/i.test(t), t.slice(0, 300));
}

// History: a journal and a version list both drew something explanatory,
// not just an empty table, when every field the core sent back was null.
{
  const { screen } = await loadPage("crates/reborn-publisher/ui/history.js", NULLS, "nulls");
  const t = text(screen);
  check("History's journal says nothing throws it blank", t.trim().length > 0, t.slice(0, 300));
}

// Keys: the trust note explains an unreadable key state instead of leaving
// the Signing keys block blank when every field is null.
{
  const { screen } = await loadPage("crates/reborn-publisher/ui/keys.js", NULLS, "nulls");
  const t = text(screen);
  check("Keys explains an unreadable signing-key state", /No signing key is configured yet|Generate one below/i.test(t), t.slice(0, 300));
}

console.log(fails === 0 ? "\nPASS: every Publisher page survives every answer" : `\nFAIL: ${fails}`);
process.exit(fails ? 1 : 0);
