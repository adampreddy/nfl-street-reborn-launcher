// "Live now" on the Online page, driven with a fake `call()` and a minimal fake DOM -- no browser,
// same pattern as Tools/check-shell-pages.mjs. Covers the fixture render (a real GET /v1/live
// answer, Tools/fixtures/live-list.json, captured from the backend against its TEST DB), the empty
// and error states, that untrusted text never becomes markup, that only the two CSP-allowed image
// prefixes are ever set as `src`, that a refresh with unchanged matches reuses the same row
// elements (keyed by match id, so it never flickers), and that polling stops while the page is
// hidden and resumes at once when it becomes visible again.
//
//     node Tools/check-live-now.mjs
import { readFileSync } from "node:fs";

const FIXTURE = JSON.parse(readFileSync("Tools/fixtures/live-list.json", "utf8"));

// ---- a DOM that records structure (see check-shell-pages.mjs) ----
function fakeEl(tag) {
  return {
    tagName: tag, children: [], className: "", textContent: "", attrs: {}, listeners: {}, hidden: false, parentNode: null,
    append(...c) { for (const x of c) { if (x && typeof x === "object") x.parentNode = this; } this.children.push(...c); },
    replaceChildren(...c) { for (const x of c) { if (x && typeof x === "object") x.parentNode = this; } this.children = c; },
    insertBefore(node, ref) {
      node.parentNode = this;
      const i = this.children.indexOf(ref);
      this.children.splice(i < 0 ? this.children.length : i, 0, node);
      return node;
    },
    addEventListener(k, f) { (this.listeners[k] ||= []).push(f); },
    removeEventListener(k, f) { this.listeners[k] = (this.listeners[k] || []).filter((x) => x !== f); },
    setAttribute(k, v) { this.attrs[k] = v; },
    querySelector() { return null; },
    querySelectorAll() { return []; },
  };
}

function text(node) {
  if (node === null || node === undefined) return "";
  if (typeof node === "string") return node;
  return [node.textContent || "", ...(node.children || []).map(text)].join(" ");
}
function findAll(node, pred, out = []) {
  if (!node || typeof node !== "object") return out;
  if (pred(node)) out.push(node);
  for (const c of node.children || []) findAll(c, pred, out);
  return out;
}
const byClass = (root, cls) => findAll(root, (n) => String(n.className || "").split(/\s+/).includes(cls));
const imgsOf = (root) => findAll(root, (n) => n.tagName === "img");
const styleOf = (n) => Object.fromEntries(String(n.attrs.style || "").split(";").filter(Boolean).map((d) => d.split(":").map((s) => s.trim())));

// WCAG relative luminance / contrast, to hold the team-colour abbreviation chip to 4.5:1 -- the
// same maths as live-color.js, kept separate here so the check verifies the real output, not its
// own assumptions about how live-color.js works.
function lum(hex) {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}
const contrast = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };

// shell.js and live-color.js are inlined the same way check-shell-pages.mjs inlines shell.js: a
// data: URL module cannot resolve a relative import, so each shared module's exports become plain
// declarations.
const SHELL = readFileSync("crates/reborn-ui/ui/shell.js", "utf8").replaceAll("export ", "");
const LIVE_COLOR = readFileSync("crates/reborn-ui/ui/live-color.js", "utf8").replaceAll("export ", "");
const IMPORT_LINE = /^import .*from ['"]\.\/(?:shell|live-color)\.js['"];\r?\n/gm;
const LIVE_NOW_SRC = readFileSync("crates/reborn-ui/ui/live-now.js", "utf8").replace(IMPORT_LINE, "");

const tick = () => new Promise((r) => setTimeout(r, 0));

/** A fresh #screen, its parent, and window.__TAURI__ wired to `invoke`, then live-now.js loaded. */
async function loadLive(invoke) {
  const screenEl = fakeEl("div#screen");
  const parent = fakeEl("div#screen-parent");
  parent.children = [screenEl];
  screenEl.parentNode = parent;
  const byId = { screen: screenEl };
  const docListeners = {};
  const doc = {
    visibilityState: "visible",
    body: { dataset: {} },
    getElementById: (id) => byId[id] ?? null,
    createElement: (tag) => fakeEl(tag),
    addEventListener: (k, f) => { (docListeners[k] ||= []).push(f); },
    removeEventListener: (k, f) => { docListeners[k] = (docListeners[k] || []).filter((x) => x !== f); },
    dispatchEvent() { return true; },
    querySelectorAll: () => [],
  };
  global.document = doc;
  global.localStorage = { getItem: () => null, setItem() {}, removeItem() {} };
  const calls = [];
  global.window = { __TAURI__: { core: { invoke: async (cmd, args) => { calls.push(cmd); return invoke(cmd, args); } } } };

  // setInterval/clearInterval are stepped by hand, exactly like check-shell-pages.mjs's
  // loadPageDriven: a real interval never waits for its own callback, and the whole point here is
  // what happens while one is still out (or stopped).
  const timers = [];
  const realSetInterval = global.setInterval, realClearInterval = global.clearInterval;
  global.setInterval = (fn) => { timers.push({ fn, alive: true }); return timers.length; };
  global.clearInterval = (id) => { if (id && timers[id - 1]) timers[id - 1].alive = false; };

  const mod = SHELL + "\n" + LIVE_COLOR + "\n" + LIVE_NOW_SRC + `\n// live-now check ${Math.random()}`;
  await import("data:text/javascript;base64," + Buffer.from(mod).toString("base64"));
  await tick(); await tick(); await tick();

  return {
    calls,
    section: () => parent.children.find((c) => c !== screenEl),
    fireVisibility(state) {
      doc.visibilityState = state;
      for (const f of docListeners.visibilitychange || []) f();
    },
    activeTimers: () => timers.filter((t) => t.alive).length,
    async fireTimers() {
      for (const t of timers) if (t.alive) await t.fn();
      await tick(); await tick();
    },
    async settle() { await tick(); await tick(); await tick(); },
    done() {
      global.setInterval = realSetInterval;
      global.clearInterval = realClearInterval;
    },
  };
}

let fails = 0;
function check(what, ok, detail = "") {
  if (!ok) fails++;
  console.log(`${ok ? "ok  " : "FAIL"}  ${what}${ok ? "" : `: ${detail}`}`);
}

// ---- 1: the real fixture, two rows, teams, score, Watch ----
{
  const invoke = async (cmd) => {
    if (cmd === "settings_paths") return {};
    if (cmd === "live_matches") return FIXTURE.matches;
    throw new Error(`no fake answer for ${cmd}`);
  };
  const page = await loadLive(invoke);
  const section = page.section();
  check("the section is inserted before #screen", !!section, "no section found beside #screen");
  const t = text(section);
  check("shows the heading", t.includes("Live now"));
  check("shows both fixture teams", t.includes("Giants") && t.includes("Colts"), t.slice(0, 400));
  check("shows the game numbering for both matches", t.includes("Game 1 of 3") && t.includes("Game 1 of 1"), t.slice(0, 400));
  check("shows both modes", t.includes("Ranked") && t.includes("Casual"));
  check("shows all four seat names", ["Adam", "Jock", "Tuna", "Riley"].every((n) => t.includes(n)), t.slice(0, 400));
  const rows = byClass(section, "live-row");
  check("renders one row per match", rows.length === FIXTURE.matches.length, `${rows.length} row(s)`);
  const watch = findAll(section, (n) => n.tagName === "a" && String(n.className || "").includes("live-watch"));
  check("every row has a Watch link", watch.length === FIXTURE.matches.length && watch.every((w) => w.textContent === "Watch"));
  check("Watch opens live.html?match=<id>",
    watch.every((w) => FIXTURE.matches.some((m) => w.attrs.href === `live.html?match=${encodeURIComponent(m.id)}`)),
    watch.map((w) => w.attrs.href).join(", "));
  const abbrevs = byClass(section, "live-abbrev");
  const wantBg = FIXTURE.matches[0].teams[0].primary;
  check("a team's abbreviation sits in a chip whose background is its own primary colour",
    abbrevs.length > 0 && abbrevs.every((a) => styleOf(a).background === wantBg), abbrevs.map((a) => a.attrs.style).join(", "));
  check("that dark navy chip (#122852) gets white text for contrast",
    abbrevs.every((a) => styleOf(a).color === "#ffffff"), abbrevs.map((a) => a.attrs.style).join(", "));
  check("the abbreviation chips clear 4.5:1 contrast",
    abbrevs.every((a) => contrast(styleOf(a).color, styleOf(a).background) >= 4.5), abbrevs.map((a) => a.attrs.style).join(", "));
  const allImgs = imgsOf(section);
  check("the fixture's logos and avatars rendered as <img>", allImgs.length > 0);
  check("only allowed image prefixes are ever set as src",
    allImgs.every((i) => i.attrs.src?.startsWith("https://api.nflstreetreborn.com/live-assets/") || i.attrs.src?.startsWith("https://cdn.discordapp.com/avatars/")),
    allImgs.map((i) => i.attrs.src).join(", "));
  page.done();
}

// ---- 2: no matches ----
{
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? [] : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  const section = page.section();
  const t = text(section);
  check("the empty state says so", t.includes("No live games right now."), t);
  check("the empty state has no rows", byClass(section, "live-row").length === 0);
  page.done();
}

// ---- 3: the backend call fails ----
{
  const MSG = "Live games couldn't be loaded right now.";
  const invoke = async (cmd) => {
    if (cmd === "settings_paths") return {};
    if (cmd === "live_matches") throw new Error(MSG);
    throw new Error(`no fake answer for ${cmd}`);
  };
  const page = await loadLive(invoke);
  const section = page.section();
  const t = text(section);
  check("the error state shows the one line", t.includes(MSG), t);
  check("the error state has no rows", byClass(section, "live-row").length === 0);
  page.done();
}

// ---- 4: a name crafted as markup lands as text, never as an element ----
{
  const HOSTILE = "<img src=x onerror=alert(1)>";
  const match = {
    id: "11111111-1111-4111-8111-111111111111", mode: "casual", best_of: 1, series: [0, 0], started_at: "2026-09-24T00:00:00Z",
    seats: [{ display_name: HOSTILE, avatar_url: null }, { display_name: "Normal", avatar_url: null }],
    game_seq: 1, score: [0, 0], sides: [{ seat: 0 }, { seat: 1 }],
    teams: [
      { side: 0, tgid: 1, name: HOSTILE, abbrev: HOSTILE, city: null, primary: "#112233", secondary: "#445566", logo_url: null, wordmark_url: null },
      { side: 1, tgid: 2, name: "Other", abbrev: "OTH", city: null, primary: "#000000", secondary: "#ffffff", logo_url: null, wordmark_url: null },
    ],
  };
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? [match] : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  const section = page.section();
  const t = text(section);
  check("a hostile name lands as literal text", t.includes(HOSTILE), t.slice(0, 400));
  check("a hostile name never becomes an element of its own", imgsOf(section).length === 0, `found ${imgsOf(section).length} <img> element(s)`);
  page.done();
}

// ---- 5: only the two allowed image prefixes are ever set as src; anything else draws a badge ----
{
  const match = {
    id: "22222222-2222-4222-8222-222222222222", mode: "ranked", best_of: 3, series: [0, 0], started_at: "2026-09-24T00:00:00Z",
    seats: [{ display_name: "A", avatar_url: "https://evil.example/a.png" }, { display_name: "B", avatar_url: "https://cdn.discordapp.com.evil.example/avatars/x.png" }],
    game_seq: 1, score: [0, 0], sides: null,
    teams: [
      { side: 0, tgid: 1, name: "Home", abbrev: "HOM", city: null, primary: "#112233", secondary: "#445566", logo_url: "https://evil.example/logo.png", wordmark_url: null },
      { side: 1, tgid: 2, name: "Away", abbrev: "AWY", city: null, primary: "#000000", secondary: "#ffffff", logo_url: "/assets/logo.png", wordmark_url: null },
    ],
  };
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? [match] : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  const section = page.section();
  check("an off-catalog logo and a non-Discord (or prefix-confused) avatar never become <img> src",
    imgsOf(section).length === 0, `found: ${imgsOf(section).map((i) => i.attrs.src).join(", ")}`);
  check("badges are drawn instead", byClass(section, "live-badge").length === 2 && byClass(section, "live-avatar-badge").length === 2);
  page.done();
}

// ---- 6: rows are keyed by match id, so an unchanged refresh never flickers ----
{
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? FIXTURE.matches : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  const before = byClass(page.section(), "live-row");
  await page.fireTimers(); // the next 5 s poll, same matches
  const after = byClass(page.section(), "live-row");
  check("a refresh with unchanged matches reuses the same row elements",
    before.length === after.length && before.length > 0 && before.every((n, i) => n === after[i]),
    "row identity changed across an unchanged refresh");
  page.done();
}

// ---- 7: polling stops while hidden, resumes (with an immediate poll) once visible again ----
{
  let liveCalls = 0;
  const invoke = async (cmd) => {
    if (cmd === "settings_paths") return {};
    if (cmd === "live_matches") { liveCalls++; return []; }
    throw new Error(`no fake answer for ${cmd}`);
  };
  const page = await loadLive(invoke);
  check("one poll happens on load", liveCalls === 1, `${liveCalls}`);
  check("one timer runs while the page is visible", page.activeTimers() === 1, `${page.activeTimers()}`);
  page.fireVisibility("hidden");
  check("the timer stops when document.visibilityState becomes hidden", page.activeTimers() === 0, `${page.activeTimers()}`);
  const callsWhileHidden = liveCalls;
  await page.settle();
  check("no poll happens while hidden", liveCalls === callsWhileHidden, `${liveCalls}`);
  page.fireVisibility("visible");
  await page.settle();
  check("becoming visible polls again at once", liveCalls === callsWhileHidden + 1, `${liveCalls}`);
  check("a new timer starts once visible again", page.activeTimers() === 1, `${page.activeTimers()}`);
  page.done();
}

// ---- 8: the abbreviation chip's background is the team's own primary; its text is chosen by luminance ----
{
  const match = {
    id: "33333333-3333-4333-8333-333333333333", mode: "ranked", best_of: 1, series: [0, 0], started_at: "2026-09-24T00:00:00Z",
    seats: [{ display_name: "A", avatar_url: null }, { display_name: "B", avatar_url: null }],
    game_seq: 1, score: [0, 0], sides: [{ seat: 0 }, { seat: 1 }],
    teams: [
      { side: 0, tgid: 1, name: "Navy", abbrev: "NVY", city: null, primary: "#0E172E", secondary: "#000000", logo_url: null, wordmark_url: null },
      { side: 1, tgid: 2, name: "Light", abbrev: "LGT", city: null, primary: "#F5D033", secondary: "#000000", logo_url: null, wordmark_url: null },
    ],
  };
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? [match] : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  const abbrevs = byClass(page.section(), "live-abbrev");
  check("two abbreviation chips are drawn", abbrevs.length === 2, `${abbrevs.length}`);
  const navy = abbrevs.find((a) => a.textContent === "NVY");
  const light = abbrevs.find((a) => a.textContent === "LGT");
  check("each chip's background is that team's own primary colour",
    !!navy && !!light && styleOf(navy).background === "#0E172E" && styleOf(light).background === "#F5D033",
    [navy, light].map((a) => a && a.attrs.style).join(" | "));
  check("a dark navy chip (#0E172E) gets white text", !!navy && styleOf(navy).color === "#ffffff", navy && navy.attrs.style);
  check("a light chip (#F5D033) gets black text", !!light && styleOf(light).color === "#000000", light && light.attrs.style);
  check("both chips clear 4.5:1 contrast",
    [navy, light].every((a) => a && contrast(styleOf(a).color, styleOf(a).background) >= 4.5),
    [navy, light].map((a) => a && a.attrs.style).join(" | "));
  page.done();
}

console.log(fails === 0
  ? "\nPASS: Live now renders the fixture's two rows, the empty and error states, escapes untrusted text, gates image origins to the two allowed prefixes, keeps row identity across a refresh, stops/resumes polling with page visibility, and gives each team's abbreviation a readable chip (its own colour as background, black or white text by luminance)."
  : `\nFAIL: ${fails}`);
process.exit(fails ? 1 : 0);
