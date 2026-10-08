// "Live now" on the Online page, driven with a fake `call()` and a minimal fake DOM -- no browser,
// same pattern as Tools/check-shell-pages.mjs. Covers the fixture render (a real GET /v1/live
// answer, Tools/fixtures/live-list.json, captured from the backend against its TEST DB), the empty
// and error states, that untrusted text never becomes markup, that only the two CSP-allowed image
// prefixes are ever set as `src`, that a refresh with unchanged matches reuses the same row
// elements (keyed by match id, so it never flickers), and that polling stops while the page is
// hidden and resumes at once when it becomes visible again.
//
// Owner 10-08: the card's link says "Live feed" and opens the site-styled match page (match.html?match=<id>&from=
// online.html); a player's name opens their profile when the list names their account.
//
// Final-review fixes (plan 4c): an unchanged list touches nothing and a focused Live feed link keeps
// focus across a changed one (I1); the error alert is replaced only when its words change (I1);
// one request at a time, 10 s polling, exponential backoff with jitter from 5 s to 60 s on errors,
// at least 60 s after a 429, last good rows kept on error (I2); players ordered by side (I3);
// polling paused while a game session runs (M7); one shared strict asset rule (M3); no live
// payload in the diagnostic log (M6).
//
//     node Tools/check-live-now.mjs
import { readFileSync } from "node:fs";

const FIXTURE = JSON.parse(readFileSync("Tools/fixtures/live-list.json", "utf8"));

// ---- a DOM that records structure (see check-shell-pages.mjs) ----
let doc = null; // the current fake document, so focus() can move document.activeElement
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
    getAttribute(k) { return k in this.attrs ? this.attrs[k] : null; },
    focus() { if (doc) doc.activeElement = this; },
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
// shell.js and live-color.js are inlined the same way check-shell-pages.mjs inlines shell.js: a
// data: URL module cannot resolve a relative import, so each shared module's exports become plain
// declarations.
const SHELL = readFileSync("crates/reborn-ui/ui/shell.js", "utf8").replaceAll("export ", "");
const NAMES = readFileSync("crates/reborn-ui/ui/names.js", "utf8").replaceAll("export ", "");
const IMPORTS = /^import .*from ['"]\.\/(?:shell|names|player-link|live-common)\.js['"];\r?\n/gm;
const PLAYER_LINK = readFileSync("crates/reborn-ui/ui/player-link.js", "utf8").replace(IMPORTS, "").replaceAll("export ", "");
const LIVE_COMMON = readFileSync("crates/reborn-ui/ui/live-common.js", "utf8").replaceAll("export ", "");
const IMPORT_LINE = IMPORTS;
const LIVE_NOW_RAW = readFileSync("crates/reborn-ui/ui/live-now.js", "utf8");
const LIVE_NOW_SRC = LIVE_NOW_RAW.replace(IMPORT_LINE, "");

// The page's timers are faked (below); the checker's own ticks use the real one.
const realSetTimeout = setTimeout, realClearTimeout = clearTimeout;
const tick = () => new Promise((r) => realSetTimeout(r, 0));

/** A fresh #screen, its parent, and window.__TAURI__ wired to `invoke`, then live-now.js loaded. */
async function loadLive(invoke) {
  const screenEl = fakeEl("div#screen");
  const parent = fakeEl("div#screen-parent");
  parent.children = [screenEl];
  screenEl.parentNode = parent;
  const outEl = fakeEl("pre#out");
  // online.html's heading count: "<span id=on-live-count hidden><i class=live-dot></i><span></span></span>".
  const countEl = fakeEl("span#on-live-count");
  const countWords = fakeEl("span");
  countEl.hidden = true;
  countEl.children = [countWords];
  countEl.querySelector = (sel) => (sel === "span" ? countWords : null);
  const byId = { screen: screenEl, out: outEl, "on-live-count": countEl };
  const docListeners = {};
  doc = {
    activeElement: null,
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

  // setTimeout/clearTimeout are stepped by hand: the page polls with a one-shot timer it
  // re-arms after each answer (so a slow answer can never overlap the next), and the whole point
  // here is what happens while one is still out (or stopped), and how long each wait is.
  const timers = [];
  global.setTimeout = (fn, ms) => { timers.push({ fn, ms, alive: true }); return timers.length; };
  global.clearTimeout = (id) => { if (id && timers[id - 1]) timers[id - 1].alive = false; };

  const mod = SHELL + "\n" + NAMES + "\n" + PLAYER_LINK + "\n" + LIVE_COMMON + "\n" + LIVE_NOW_SRC + `\n// live-now check ${Math.random()}`;
  await import("data:text/javascript;base64," + Buffer.from(mod).toString("base64"));
  await tick(); await tick(); await tick();

  return {
    calls,
    doc,
    out: outEl,
    count: countEl,
    countWords,
    section: () => parent.children.find((c) => c !== screenEl),
    fireVisibility(state) {
      doc.visibilityState = state;
      for (const f of docListeners.visibilitychange || []) f();
    },
    fireSession(active) { for (const f of docListeners["session-state"] || []) f({ detail: active }); },
    activeTimers: () => timers.filter((t) => t.alive).length,
    nextWait: () => timers.filter((t) => t.alive).map((t) => t.ms),
    async fireTimers() {
      for (const t of [...timers]) if (t.alive) { t.alive = false; await t.fn(); }
      await tick(); await tick();
    },
    async settle() { await tick(); await tick(); await tick(); },
    done() {
      global.setTimeout = realSetTimeout;
      global.clearTimeout = realClearTimeout;
    },
  };
}

let fails = 0;
function check(what, ok, detail = "") {
  if (!ok) fails++;
  console.log(`${ok ? "ok  " : "FAIL"}  ${what}${ok ? "" : `: ${detail}`}`);
}

// ---- 1: the real fixture, two rows, teams, score, Live feed ----
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
  check("shows the mode line for both matches (spec 2026-10-02 §5)", t.includes("Ranked · Game 1") && t.includes("Casual"), t.slice(0, 400));
  check("shows both modes", t.includes("Ranked") && t.includes("Casual"));
  check("shows all four seat names", ["Adam", "Jock", "Tuna", "Riley"].every((n) => t.includes(n)), t.slice(0, 400));
  const rows = byClass(section, "live-row");
  check("renders one row per match", rows.length === FIXTURE.matches.length, `${rows.length} row(s)`);
  const watch = findAll(section, (n) => n.tagName === "a" && String(n.className || "").includes("live-watch"));
  check("every row has a Live feed link", watch.length === FIXTURE.matches.length && watch.every((w) => w.textContent === "Live feed"));
  check("no card says Watch any more", !text(section).includes("Watch"));
  check("Live feed opens the match page, following the match, Back to Online",
    watch.every((w) => FIXTURE.matches.some((m) => w.attrs.href === `match.html?match=${encodeURIComponent(m.id)}&from=online.html`)),
    watch.map((w) => w.attrs.href).join(", "));
  const sides = byClass(section, "live-side");
  check("each side's fade is its own team's validated primary colour (--tc)",
    sides.length === 2 * FIXTURE.matches.length && sides.every((s) => /^--tc:#[0-9a-fA-F]{6}$/.test(s.attrs.style || "")), sides.map((s) => s.attrs.style).join(", "));
  const allImgs = imgsOf(section);
  check("the fixture's logos rendered as <img>", allImgs.length > 0);
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
  check("the empty state says so", t.includes("Streets are quiet right now."), t);
  check("the empty state has no rows", byClass(section, "live-row").length === 0);
  page.done();
}

// ---- 2b: the heading's live-games count shows only while games are live; it hides again when the list drops to 0 ----
{
  const answers = [FIXTURE.matches, []];
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? answers.shift() ?? [] : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  check("a list of two shows \"2 live games\" in the heading",
    page.count.hidden === false && page.countWords.textContent === "2 live games", `hidden=${page.count.hidden} "${page.countWords.textContent}"`);
  await page.fireTimers();
  check("the list dropping to 0 hides the count (never \"0 live games\")", page.count.hidden === true, `hidden=${page.count.hidden} "${page.countWords.textContent}"`);
  page.done();
}
{
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? [] : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  check("an empty first answer leaves the count hidden and unwritten", page.count.hidden === true && page.countWords.textContent === "", `hidden=${page.count.hidden} "${page.countWords.textContent}"`);
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
  check("badges are drawn instead (the card draws no avatars)", byClass(section, "live-badge").length === 2 && byClass(section, "live-avatar").length === 0);
  page.done();
}

// ---- 6: rows are keyed by match id, so an unchanged refresh never flickers ----
{
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? FIXTURE.matches : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  const before = byClass(page.section(), "live-row");
  await page.fireTimers(); // the next 10 s poll, same matches
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

// ---- 8: a team's primary becomes its side's fade (--tc) only when it is a valid #rrggbb ----
{
  const match = {
    id: "33333333-3333-4333-8333-333333333333", mode: "ranked", best_of: 1, series: [0, 0], started_at: "2026-09-24T00:00:00Z",
    seats: [{ display_name: "A", avatar_url: null }, { display_name: "B", avatar_url: null }],
    game_seq: 1, score: [7, 3], sides: [{ seat: 0 }, { seat: 1 }],
    teams: [
      { side: 0, tgid: 1, name: "Navy", abbrev: "NVY", city: null, primary: "#0E172E", secondary: "#000000", logo_url: null, wordmark_url: null },
      { side: 1, tgid: 2, name: "Odd", abbrev: "ODD", city: null, primary: "red;background:url(x)", secondary: "#000000", logo_url: null, wordmark_url: null },
    ],
  };
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? [match] : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  const [navy, odd] = byClass(page.section(), "live-side");
  check("a valid primary becomes the side's --tc", !!navy && navy.attrs.style === "--tc:#0E172E", navy && navy.attrs.style);
  check("anything else sets no style at all", !!odd && odd.attrs.style === undefined, odd && odd.attrs.style);
  check("the losing side is dimmed", !!odd && String(odd.className).split(/\s+/).includes("lose") && !String(navy.className).split(/\s+/).includes("lose"));
  page.done();
}

// ---- 9 (I1): an unchanged list touches nothing; a changed one keeps a focused Live feed link focused ----
{
  let answer = FIXTURE.matches;
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? answer : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  const watchOf = () => findAll(page.section(), (n) => n.tagName === "a" && String(n.className || "").includes("live-watch"));
  const first = watchOf();
  check("every Live feed link has a data-key naming its match",
    first.length === FIXTURE.matches.length && first.every((w, i) => w.attrs["data-key"] === `feed-${FIXTURE.matches[i].id}`),
    first.map((w) => w.attrs["data-key"]).join(", "));
  first[1].focus();
  await page.fireTimers();
  const same = watchOf();
  check("an unchanged list redraws nothing (the very same Live feed links stay)", same.every((w, i) => w === first[i]));
  check("focus is untouched by an unchanged poll", page.doc.activeElement === first[1]);
  answer = JSON.parse(JSON.stringify(FIXTURE.matches));
  answer[0].score = [7, 0];
  answer[1].score = [0, 3];
  await page.fireTimers();
  const changed = watchOf();
  check("a changed list redraws its rows", text(page.section()).includes("7") && changed[1] !== first[1]);
  check("the Live feed link that had focus keeps it across the redraw (the new link with the same data-key)",
    page.doc.activeElement === changed[1] && changed[1].attrs["data-key"] === first[1].attrs["data-key"],
    `active: ${page.doc.activeElement && page.doc.activeElement.attrs && page.doc.activeElement.attrs["data-key"]}`);
  page.done();
}

// ---- 10 (I1, I2): errors keep the last good rows; the alert is replaced only when its words change ----
{
  const MSG = "Live games couldn't be loaded right now.";
  const MSG2 = "Live games are busy right now. Trying again in a minute.";
  let mode = "ok";
  const invoke = async (cmd) => {
    if (cmd === "settings_paths") return {};
    if (cmd === "live_matches") {
      if (mode === "fail") throw { kind: "unavailable", message: MSG };
      if (mode === "limit") throw { kind: "rate_limited", message: MSG2 };
      return FIXTURE.matches;
    }
    throw new Error(`no fake answer for ${cmd}`);
  };
  const page = await loadLive(invoke);
  const rowsBefore = byClass(page.section(), "live-row");
  check("polls every 10 s after a good answer", page.nextWait().length === 1 && page.nextWait()[0] === 10000, JSON.stringify(page.nextWait()));
  mode = "fail";
  await page.fireTimers();
  const alerts1 = findAll(page.section(), (n) => n.attrs && n.attrs.role === "alert");
  check("an error shows its one line", alerts1.length === 1 && alerts1[0].textContent === MSG, alerts1.map((a) => a.textContent).join("|"));
  const rowsAfter = byClass(page.section(), "live-row");
  check("an error keeps the last good rows (the same elements)", rowsAfter.length === rowsBefore.length && rowsBefore.length > 0 && rowsAfter.every((r, i) => r === rowsBefore[i]), `${rowsAfter.length} row(s)`);
  const w1 = page.nextWait()[0];
  check("the first error backs off 5 s plus jitter (5-6.25 s)", w1 >= 5000 && w1 <= 6250, `${w1}`);
  await page.fireTimers();
  const alerts2 = findAll(page.section(), (n) => n.attrs && n.attrs.role === "alert");
  check("the same error words leave the very same alert in place", alerts2.length === 1 && alerts2[0] === alerts1[0]);
  const w2 = page.nextWait()[0];
  check("a second error in a row doubles the wait (10-12.5 s)", w2 >= 10000 && w2 <= 12500, `${w2}`);
  for (let i = 0; i < 6; i++) await page.fireTimers();
  const wMax = page.nextWait()[0];
  check("the backoff caps at 60 s", wMax <= 60000 && wMax >= 40000, `${wMax}`);
  mode = "limit";
  await page.fireTimers();
  const alerts3 = findAll(page.section(), (n) => n.attrs && n.attrs.role === "alert");
  check("new error words replace the alert", alerts3.length === 1 && alerts3[0] !== alerts1[0] && alerts3[0].textContent === MSG2);
  mode = "ok";
  await page.fireTimers();
  check("a good answer clears the error line", findAll(page.section(), (n) => n.attrs && n.attrs.role === "alert").length === 0);
  check("a good answer resets to the 10 s poll", page.nextWait()[0] === 10000, JSON.stringify(page.nextWait()));
  check("the rows are still the same elements after recovering", byClass(page.section(), "live-row").every((r, i) => r === rowsBefore[i]));
  page.done();
}

// ---- 11 (I2): a 429 waits at least 60 s, even on the first error ----
{
  const invoke = async (cmd) => {
    if (cmd === "settings_paths") return {};
    if (cmd === "live_matches") throw { kind: "rate_limited", message: "Live games are busy right now. Trying again in a minute." };
    throw new Error(`no fake answer for ${cmd}`);
  };
  const page = await loadLive(invoke);
  check("a rate limit (kind rate_limited) waits at least 60 s", page.nextWait().length === 1 && page.nextWait()[0] >= 60000, JSON.stringify(page.nextWait()));
  const n = page.calls.filter((c) => c === "live_matches").length;
  page.fireVisibility("hidden"); page.fireVisibility("visible");
  await page.settle();
  check("showing the page again does not skip the rate-limit wait", page.calls.filter((c) => c === "live_matches").length === n,
    `${page.calls.filter((c) => c === "live_matches").length - n} extra call(s)`);
  page.done();
}

// ---- 12 (I2): one request at a time ----
{
  let release = null;
  let liveCalls = 0;
  const invoke = (cmd) => {
    if (cmd === "settings_paths") return Promise.resolve({});
    if (cmd === "live_matches") { liveCalls++; return new Promise((r) => { release = () => r(FIXTURE.matches); }); }
    return Promise.reject(new Error(`no fake answer for ${cmd}`));
  };
  const page = await loadLive(invoke);
  check("the first poll is out", liveCalls === 1, `${liveCalls}`);
  page.fireVisibility("hidden"); page.fireVisibility("visible");
  page.fireSession(false);
  await page.fireTimers();
  check("nothing asks again while a poll is still out", liveCalls === 1, `${liveCalls}`);
  check("no timer is armed while a poll is out", page.activeTimers() === 0, `${page.activeTimers()}`);
  release();
  await page.settle();
  check("the answer arms exactly one next poll", page.activeTimers() === 1, `${page.activeTimers()}`);
  page.done();
}

// ---- 13 (I3): each row's players follow the teams' sides ----
{
  const swapped = JSON.parse(JSON.stringify(FIXTURE.matches[0]));
  swapped.sides = [{ seat: 1 }, { seat: 0 }]; // seat 1 (Jock) has side 0 (Giants)
  const noSides = JSON.parse(JSON.stringify(FIXTURE.matches[0]));
  noSides.id = "44444444-4444-4444-8444-444444444444";
  noSides.sides = null;
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? [swapped, noSides] : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  const [r0, r1] = byClass(page.section(), "live-row");
  const names = (r) => byClass(r, "live-player-name").map((n) => n.textContent);
  check("seat 0 on side 1: the side-0 team's player (Jock) is listed first", names(r0).join(",") === "Jock,Adam", names(r0).join(","));
  check("unknown sides fall back to seat order", names(r1).join(",") === "Adam,Jock", names(r1).join(","));
  page.done();
}

// ---- 13b: a real g5-shaped bug -- ports [-1, 0], a CPU side never borrows the other side's seat ----
{
  const mixed = JSON.parse(JSON.stringify(FIXTURE.matches[0]));
  mixed.sides = [{ seat: null }, { seat: 0 }]; // side 0 (Giants) is CPU; side 1's (Colts) only human is seat 0, Adam
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? [mixed] : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  const [row] = byClass(page.section(), "live-row");
  const names = byClass(row, "live-player-name").map((n) => n.textContent);
  check("side 0 (CPU, no seat) shows CPU, side 1 shows seat 0's own name -- in that order, by side",
    names.join(",") === "CPU,Adam", names.join(","));
  check("Jock's name never appears (seat 1 is not part of this game)", !text(row).includes("Jock"), text(row));
  check("the CPU side still shows its own team and score", text(row).includes("Giants") && text(row).includes("Colts"), text(row).slice(0, 300));
  page.done();
}

// ---- 13c (owner 10-08): a player with an account id is a link to their profile; one without stays plain ----
{
  const named = JSON.parse(JSON.stringify(FIXTURE.matches[0]));
  named.seats[0].player_id = "0A1B2C3D-4E5F-4A6B-8C7D-9E0F1A2B3C4D"; // the server's case does not matter
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? [named] : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  const [row] = byClass(page.section(), "live-row");
  const links = findAll(row, (n) => n.tagName === "a" && String(n.className || "").includes("player-link"));
  check("Adam's name is a link to his profile in the launcher",
    links.length === 1 && links[0].textContent === "Adam" && links[0].attrs.href === "profile.html?player=0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d",
    links.map((l) => `${l.textContent} ${l.attrs.href}`).join(", "));
  const plain = byClass(row, "live-player-name").filter((n) => n.tagName === "em");
  check("Jock (no account id in the answer) stays plain text", plain.length === 1 && plain[0].textContent === "Jock", plain.map((n) => n.textContent).join(","));
  page.done();
}

// ---- 14 (M7): polling pauses while a game session runs ----
{
  let liveCalls = 0;
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? (liveCalls++, []) : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  page.fireSession(true);
  check("a running game stops the timer", page.activeTimers() === 0, `${page.activeTimers()}`);
  const n = liveCalls;
  page.fireSession(true);
  page.fireVisibility("hidden"); page.fireVisibility("visible");
  await page.settle();
  check("no poll while the game runs, even when the page is shown again", liveCalls === n, `${liveCalls - n}`);
  page.fireSession(false);
  await page.settle();
  check("the game ending polls again at once", liveCalls === n + 1, `${liveCalls - n}`);
  check("and re-arms the timer", page.activeTimers() === 1, `${page.activeTimers()}`);
  page.fireSession(false);
  await page.settle();
  check("a repeated session-state (false) does not poll again", liveCalls === n + 1, `${liveCalls - n}`);
  page.done();
}

// ---- 15 (M3, M6): the one strict asset rule, and no live payload in the diagnostic log ----
{
  check("live-now.js takes its image rules from live-common.js", /import \{[^}]*\bassetUrl\b[^}]*\} from '\.\/live-common\.js'/.test(LIVE_NOW_RAW));
  check("live-now.js has no image-prefix rule of its own", !/live-assets|cdn\.discordapp/.test(LIVE_NOW_RAW.replace(/^\s*\/\/.*$/gm, "")));
  const match = JSON.parse(JSON.stringify(FIXTURE.matches[0]));
  match.teams[0].logo_url = "/live-assets/r/../../secret.png";
  match.teams[1].logo_url = "/live-assets/r/x/teams/10-256.svg";
  const invoke = async (cmd) => (cmd === "settings_paths" ? {} : cmd === "live_matches" ? [match] : Promise.reject(new Error(`no fake answer for ${cmd}`)));
  const page = await loadLive(invoke);
  check("a traversal or non-png /live-assets/ path draws its badge, not an <img>",
    byClass(page.section(), "live-logo").length === 0 && byClass(page.section(), "live-badge").length === 2,
    byClass(page.section(), "live-logo").map((i) => i.attrs.src).join(", "));
  await page.fireTimers();
  check("polling never writes a live payload into the diagnostic log", !page.out.textContent.includes("live_matches") && !page.out.textContent.includes("Giants"),
    page.out.textContent.slice(0, 200));
  page.done();
}

console.log(fails === 0
  ? "\nPASS: Live now renders the fixture's two rows, the empty and error states, escapes untrusted text, gates image origins to the two allowed prefixes, keeps row identity across a refresh, stops/resumes polling with page visibility, and fades each side in its own validated team colour."
  : `\nFAIL: ${fails}`);
process.exit(fails ? 1 : 0);
