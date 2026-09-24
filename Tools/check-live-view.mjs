// The native match view (live.html + live.js), driven with a fake `call()` and a minimal fake DOM -- no browser,
// same pattern as Tools/check-live-now.mjs. The fixture, Tools/fixtures/live-snapshot.json, is a real
// `GET /v1/live/:id` answer captured from the backend against its TEST DB: game 1 recorded and verified (no live
// plays, its box score from the record), game 2 live, whose play 5 is DISPUTED (seat 0's first copy claims a
// touchdown, seat 1's copy disagrees), so its points never reach the score.
//
// Covers: the header (teams, seats, score, Live, Game N of M, series, the situation of the newest NON-disputed
// play); picking game 1 (Final, its score, "Final box score"); a disputed play (badge, no score highlight); a
// snapshot with no teams or sides (seat names, "Side 1 x – y Side 2", no <img> at a missing URL); a long or HTML
// name stays literal; a match that turns `complete` shows Final and stops polling; an invalid ?match= shows the
// not-found line and never asks for a snapshot; Watch on web and Back; polling every 3 s only while visible.
//
//     node Tools/check-live-view.mjs
import { readFileSync } from "node:fs";

const FIXTURE_TEXT = readFileSync("Tools/fixtures/live-snapshot.json", "utf8");
const fixture = () => JSON.parse(FIXTURE_TEXT);
const FIX = fixture();
const MATCH_ID = FIX.match.id;

// ---- a DOM that records structure (see check-live-now.mjs) ----
function fakeEl(tag) {
  return {
    tagName: tag, children: [], className: "", textContent: "", attrs: {}, listeners: {}, hidden: false, parentNode: null,
    append(...c) { for (const x of c) { if (x && typeof x === "object") x.parentNode = this; } this.children.push(...c); },
    appendChild(x) { this.append(x); return x; },
    replaceChildren(...c) { for (const x of c) { if (x && typeof x === "object") x.parentNode = this; } this.children = c; },
    addEventListener(k, f) { (this.listeners[k] ||= []).push(f); },
    removeEventListener(k, f) { this.listeners[k] = (this.listeners[k] || []).filter((x) => x !== f); },
    setAttribute(k, v) { this.attrs[k] = String(v); },
    getAttribute(k) { return k in this.attrs ? this.attrs[k] : null; },
    focus() {},
    querySelector() { return null; },
    querySelectorAll() { return []; },
  };
}

function text(node) {
  if (node === null || node === undefined) return "";
  if (typeof node === "string") return node;
  if (node.hidden) return "";
  return [node.textContent || "", ...(node.children || []).map(text)].join(" ");
}
function findAll(node, pred, out = []) {
  if (!node || typeof node !== "object") return out;
  if (pred(node)) out.push(node);
  for (const c of node.children || []) findAll(c, pred, out);
  return out;
}
const classes = (n) => String(n.className || "").split(/\s+/);
const byClass = (root, cls) => findAll(root, (n) => classes(n).includes(cls));
const imgsOf = (root) => findAll(root, (n) => n.tagName === "img");
const buttonsOf = (root) => findAll(root, (n) => n.tagName === "button");
const button = (root, label) => buttonsOf(root).find((b) => text(b).trim() === label);
async function click(node) { for (const f of node?.listeners?.click || []) await f({ preventDefault() {} }); await tick(); await tick(); }

const SHELL = readFileSync("crates/reborn-ui/ui/shell.js", "utf8").replaceAll("export ", "");
const LIVE_COLOR = readFileSync("crates/reborn-ui/ui/live-color.js", "utf8").replaceAll("export ", "");
const IMPORT_LINE = /^import .*from ['"]\.\/(?:shell|live-color)\.js['"];\r?\n/gm;
const LIVE_SRC = readFileSync("crates/reborn-ui/ui/live.js", "utf8").replace(IMPORT_LINE, "");
const LIVE_HTML = readFileSync("crates/reborn-ui/ui/live.html", "utf8");

const tick = () => new Promise((r) => setTimeout(r, 0));

/** A fresh #screen, `location.search`, window.__TAURI__ wired to `invoke`, then live.js loaded. */
async function loadLive(search, invoke) {
  const screenEl = fakeEl("section#screen");
  const byId = { screen: screenEl };
  const docListeners = {};
  const winListeners = {};
  const doc = {
    visibilityState: "visible",
    body: { dataset: {}, className: "shell ob page-online" },
    title: "",
    getElementById: (id) => byId[id] ?? null,
    createElement: (tag) => fakeEl(tag),
    addEventListener: (k, f) => { (docListeners[k] ||= []).push(f); },
    removeEventListener: (k, f) => { docListeners[k] = (docListeners[k] || []).filter((x) => x !== f); },
    querySelectorAll: () => [],
  };
  global.document = doc;
  global.localStorage = { getItem: () => null, setItem() {}, removeItem() {} };
  global.location = { pathname: "/live.html", search };
  const calls = [];
  global.window = {
    __TAURI__: { core: { invoke: async (cmd, args) => { calls.push({ cmd, args }); return invoke(cmd, args); } } },
    addEventListener: (k, f) => { (winListeners[k] ||= []).push(f); },
    removeEventListener: (k, f) => { winListeners[k] = (winListeners[k] || []).filter((x) => x !== f); },
  };
  global.addEventListener = global.window.addEventListener;

  const timers = [];
  const realSetInterval = global.setInterval, realClearInterval = global.clearInterval;
  global.setInterval = (fn, ms) => { timers.push({ fn, ms, alive: true }); return timers.length; };
  global.clearInterval = (id) => { if (id && timers[id - 1]) timers[id - 1].alive = false; };

  const mod = SHELL + "\n" + LIVE_COLOR + "\n" + LIVE_SRC + `\n// live view check ${Math.random()}`;
  await import("data:text/javascript;base64," + Buffer.from(mod).toString("base64"));
  for (let i = 0; i < 6; i++) await tick();

  return {
    calls,
    screen: screenEl,
    liveCalls: () => calls.filter((c) => c.cmd === "live_match"),
    fireVisibility(state) {
      doc.visibilityState = state;
      for (const f of docListeners.visibilitychange || []) f();
    },
    firePagehide() { for (const f of winListeners.pagehide || []) f(); },
    activeTimers: () => timers.filter((t) => t.alive),
    async fireTimers() {
      for (const t of timers) if (t.alive) await t.fn();
      for (let i = 0; i < 4; i++) await tick();
    },
    async settle() { for (let i = 0; i < 4; i++) await tick(); },
    done() { global.setInterval = realSetInterval; global.clearInterval = realClearInterval; },
  };
}

const answering = (snap) => async (cmd) => {
  if (cmd === "settings_paths") return {};
  if (cmd === "live_match") return typeof snap === "function" ? snap() : snap;
  if (cmd === "open_website") return null;
  throw new Error(`no fake answer for ${cmd}`);
};

let fails = 0;
function check(what, ok, detail = "") {
  if (!ok) fails++;
  console.log(`${ok ? "ok  " : "FAIL"}  ${what}${ok ? "" : `: ${detail}`}`);
}

const ALLOWED = (src) => typeof src === "string" && src.length > 0
  && (src.startsWith("https://api.nflstreetreborn.com/live-assets/") || src.startsWith("https://cdn.discordapp.com/avatars/"));

// WCAG relative luminance / contrast, to hold the team-colour name to 4.5:1.
function lum(hex) {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}
const contrast = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p); return (x + 0.05) / (y + 0.05); };
const styleOf = (n) => Object.fromEntries(String(n.attrs.style || "").split(";").filter(Boolean).map((d) => d.split(":").map((s) => s.trim())));

// ---- 0: the page shell ----
{
  check("live.html loads live.js as a module", /<script type="module" src="live\.js"><\/script>/.test(LIVE_HTML));
  check("live.html has the shared shell (app-shell.js, the nav, #screen)",
    LIVE_HTML.includes('src="app-shell.js"') && LIVE_HTML.includes('class="shell-nav"') && LIVE_HTML.includes('id="screen"'));
  check("live.html marks Online as the current nav page", /<a class="current" href="online\.html"/.test(LIVE_HTML));
  check("live.html loads live-view.css", LIVE_HTML.includes('href="live-view.css"'));
  check("live.js never uses innerHTML or insertAdjacentHTML", !/innerHTML|outerHTML|insertAdjacentHTML/.test(LIVE_SRC));
}

// ---- 1: the header from the real fixture (game 2, live, newest) ----
{
  const page = await loadLive(`?match=${MATCH_ID}`, answering(fixture()));
  const s = page.screen;
  const header = byClass(s, "lv-header")[0];
  check("a header is drawn", !!header, text(s).slice(0, 300));
  const h = text(header);
  check("the header names both teams", h.includes("Giants") && h.includes("Colts"), h.slice(0, 400));
  check("the header names both seats", h.includes("Adam") && h.includes("Jock"), h.slice(0, 400));
  check("the header says Live", byClass(header, "lv-status").some((n) => n.textContent === "Live"), h);
  check("the header says Game 2 of 3", h.includes("Game 2 of 3"), h);
  check("the header shows the series", h.includes("Series 1–0"), h);
  const pts = byClass(header, "lv-points").map((n) => n.textContent);
  check("the header shows the running score (the disputed TD does not count)", pts.join(",") === "0,0", pts.join(","));
  const sit = byClass(header, "lv-situation").map((n) => n.textContent);
  check("the situation is the newest NON-disputed play's", sit.length === 1 && sit[0] === "4th & 15 at own 5", sit.join("|"));
  check("the disputed play's situation never reaches the header", !h.includes("1st & 2 at opp 2"));
  const logos = byClass(header, "lv-logo");
  check("each team's 256 px logo is an <img> under our live-assets", logos.length === 2 && logos.every((i) => i.tagName === "img" && ALLOWED(i.attrs.src) && /-256\.png$/.test(i.attrs.src)),
    logos.map((i) => i.attrs.src).join(", "));
  const avatars = byClass(header, "lv-avatar");
  check("each seat's Discord avatar is an <img>", avatars.length === 2 && avatars.every((i) => i.tagName === "img" && ALLOWED(i.attrs.src)));
  const names = byClass(header, "lv-team-name");
  check("team names sit in their team colour", names.length === 2 && names.every((n) => styleOf(n).background === "#122852"), names.map((n) => n.attrs.style).join(" | "));
  check("team-colour names are black or white text at 4.5:1 or better",
    names.every((n) => { const st = styleOf(n); return ["#000000", "#ffffff"].includes(st.color) && contrast(st.color, st.background) >= 4.5; }),
    names.map((n) => n.attrs.style).join(" | "));
  check("every <img> on the page is an allowed URL", imgsOf(s).every((i) => ALLOWED(i.attrs.src)), imgsOf(s).map((i) => i.attrs.src).join(", "));
  check("it asked for exactly this match", page.liveCalls().length === 1 && page.liveCalls()[0].args?.matchId === MATCH_ID, JSON.stringify(page.liveCalls()));
  check("a picker appears for a two-game match", !!button(s, "Game 1") && !!button(s, "Game 2"));
  check("the newest game is picked by default", button(s, "Game 2")?.attrs["aria-pressed"] === "true");
  check("Plays, Box score and Drives tabs exist", ["Plays", "Box score", "Drives"].every((l) => !!button(s, l)));
  check("it polls every 3 s while visible", page.activeTimers().length === 1 && page.activeTimers()[0].ms === 3000, JSON.stringify(page.activeTimers().map((t) => t.ms)));

  // Plays: newest first, the disputed play has its badge and no score highlight.
  const plays = byClass(s, "lv-play");
  check("the Plays tab lists every play of game 2, newest first", plays.length === 5 && text(plays[0]).includes("#5") && text(plays[4]).includes("#1"),
    plays.map((p) => text(p).slice(0, 30)).join(" / "));
  const disputed = plays.filter((p) => classes(p).includes("disputed"));
  check("exactly one play is disputed", disputed.length === 1, `${disputed.length}`);
  check("the disputed play carries its badge", disputed.length === 1 && byClass(disputed[0], "lv-badge").some((b) => b.textContent === "Disputed"));
  check("the disputed TD gets no score highlight", disputed.length === 1 && !classes(disputed[0]).includes("score"));
  check("the disputed TD gets no portrait", disputed.length === 1 && imgsOf(disputed[0]).length === 0);

  // Watch on web and Back.
  const watch = button(s, "Watch on web");
  check("a Watch on web button exists", !!watch);
  await click(watch);
  const web = page.calls.filter((c) => c.cmd === "open_website");
  check("Watch on web opens the backend's live page for this match",
    web.length === 1 && web[0].args?.url === `https://api.nflstreetreborn.com/live/${MATCH_ID}`, JSON.stringify(web));
  const back = findAll(s, (n) => n.tagName === "a" && n.attrs.href === "online.html");
  check("Back returns to Online", back.length === 1 && text(back[0]).includes("Back"));

  // Box score (provisional) with small portraits.
  await click(button(s, "Box score"));
  const box = byClass(s, "lv-box")[0];
  const bt = text(box);
  check("a live game's box score says (provisional)", bt.includes("(provisional)") && !bt.includes("Final box score"), bt.slice(0, 200));
  check("the box score has rushing and defense tables", bt.includes("Rushing") && bt.includes("Defense"), bt.slice(0, 300));
  check("the box score names the players", bt.includes("Jonathan Taylor") && bt.includes("Cam Skattebo"));
  const faces = byClass(box, "lv-face");
  check("box score rows carry small portraits from live-assets", faces.length > 0 && faces.every((i) => i.tagName === "img" && ALLOWED(i.attrs.src) && /\/portraits\//.test(i.attrs.src)),
    faces.map((i) => i.attrs.src).join(", "));

  await click(button(s, "Drives"));
  const dt = text(byClass(s, "lv-drives")[0]);
  check("the Drives tab lists the drives", dt.includes("Drives (provisional)") && dt.includes("Colts"), dt.slice(0, 300));

  // Picking game 1: Final, its score, the final box score; the header follows the pick.
  await click(button(s, "Game 1"));
  const h1 = text(byClass(s, "lv-header")[0]);
  check("picking game 1 shows Final", byClass(s, "lv-status").some((n) => n.textContent === "Final"), h1);
  check("picking game 1 shows Game 1 of 3", h1.includes("Game 1 of 3"), h1);
  check("a Final game shows no situation", byClass(s, "lv-situation").length === 0);
  check("the picker marks game 1", button(s, "Game 1")?.attrs["aria-pressed"] === "true");
  await click(button(s, "Box score"));
  const b1 = text(byClass(s, "lv-box")[0]);
  check("game 1's box score says Final box score", b1.includes("Final box score") && !b1.includes("(provisional)"), b1.slice(0, 200));
  await click(button(s, "Plays"));
  check("game 1 (recorded, no live plays) says so on Plays", text(s).includes("No live plays reached us for this game."));
  // A refresh keeps the pick.
  await page.fireTimers();
  check("a refresh keeps the picked game", text(byClass(s, "lv-header")[0]).includes("Game 1 of 3"));
  page.done();
}

// ---- 2: the header follows the picked game's own score ----
{
  const snap = fixture();
  snap.games[0].score = [21, 14];
  const page = await loadLive(`?match=${MATCH_ID}`, answering(snap));
  const pts = () => byClass(page.screen, "lv-points").map((n) => n.textContent).join(",");
  check("before picking, the header shows game 2's score", pts() === "0,0", pts());
  await click(button(page.screen, "Game 1"));
  check("after picking game 1, the header shows game 1's Final score", pts() === "21,14", pts());
  page.done();
}

// ---- 3: a scoring play and a big play: highlight and portrait ----
{
  const snap = fixture();
  const p4 = snap.games[1].plays[3];
  p4.body.summary = { ...p4.body.summary, rusher: 50, yards: 25, touchdown: true, points: 6, text: "Jonathan Taylor run for 25 yds. TOUCHDOWN." };
  const page = await loadLive(`?match=${MATCH_ID}`, answering(snap));
  const play = byClass(page.screen, "lv-play").find((p) => text(p).includes("#4"));
  check("a counted scoring play is highlighted", play && classes(play).includes("score"), play && play.className);
  const face = play && imgsOf(play)[0];
  check("a big play shows its player's portrait", !!face && ALLOWED(face.attrs.src) && face.attrs.src.includes("/portraits/153-"), face && face.attrs.src);
  page.done();
}

// ---- 4: no teams and no sides ----
{
  const snap = fixture();
  for (const g of snap.games) { delete g.teams; delete g.sides; }
  const page = await loadLive(`?match=${MATCH_ID}`, answering(snap));
  const s = page.screen;
  const h = text(byClass(s, "lv-header")[0]);
  check("with no sides, the header still names both seats", h.includes("Adam") && h.includes("Jock"), h.slice(0, 300));
  const side = byClass(s, "lv-side-score").map((n) => n.textContent);
  check("with no sides, the score is labelled by side", side.length === 1 && side[0] === "Side 1 0 – 0 Side 2", side.join("|"));
  check("with no teams, no logo <img> is drawn", byClass(s, "lv-logo").length === 0);
  check("with no teams, no <img> points at a missing URL", imgsOf(s).every((i) => ALLOWED(i.attrs.src)), imgsOf(s).map((i) => i.attrs.src).join(", "));
  check("with no teams, no team names are invented", !h.includes("Giants") && !h.includes("undefined") && !h.includes("null"), h.slice(0, 300));
  page.done();
}

// ---- 5: long and HTML names stay literal ----
{
  const HOSTILE = "<img src=x onerror=alert(1)>";
  const LONG = "L".repeat(300);
  const snap = fixture();
  snap.seats[0].display_name = HOSTILE;
  snap.seats[0].avatar_url = "javascript:alert(1)";
  snap.seats[1].display_name = LONG;
  snap.games[1].teams[0].name = `<b>${HOSTILE}</b>`;
  snap.games[1].teams[0].primary = "red;background:url(x)";
  snap.games[1].teams[0].logo_url = "https://evil.example/logo.png";
  snap.games[1].plays[0].body.summary.text = `<script>alert(1)</script>`;
  snap.games[1].box.lines[0].name = HOSTILE;
  snap.games[1].box.lines[0].portrait_url = "https://evil.example/p.png";
  const page = await loadLive(`?match=${MATCH_ID}`, answering(snap));
  const s = page.screen;
  const t = text(s);
  check("an HTML display name renders as literal text", t.includes(HOSTILE));
  check("a long display name renders whole", t.includes(LONG));
  check("an HTML team name renders as literal text", t.includes(`<b>${HOSTILE}</b>`));
  check("an HTML play summary renders as literal text", t.includes("<script>alert(1)</script>"));
  check("no element is created from any of it", findAll(s, (n) => n.tagName === "script" || n.tagName === "b").length === 0);
  check("only allowed URLs become <img> src", imgsOf(s).every((i) => ALLOWED(i.attrs.src)), imgsOf(s).map((i) => i.attrs.src).join(", "));
  const names = byClass(s, "lv-team-name");
  check("a colour that is not a plain hex is never written into a style", names.every((n) => !String(n.attrs.style || "").includes("url(")), names.map((n) => n.attrs.style).join(" | "));
  page.done();
}

// ---- 6: a match that turns complete shows Final and stops polling ----
{
  let status = "running";
  const page = await loadLive(`?match=${MATCH_ID}`, answering(() => { const x = fixture(); x.match.status = status; return x; }));
  check("while running, the header says Live", byClass(page.screen, "lv-status").some((n) => n.textContent === "Live"));
  status = "complete";
  await page.fireTimers();
  check("once complete, the header says Final", byClass(page.screen, "lv-status").some((n) => n.textContent === "Final"), text(byClass(page.screen, "lv-header")[0]));
  check("once complete and drawn, polling stops", page.activeTimers().length === 0, `${page.activeTimers().length}`);
  const n = page.liveCalls().length;
  page.fireVisibility("hidden"); page.fireVisibility("visible");
  await page.settle();
  check("a finished match does not start polling again when shown", page.liveCalls().length === n && page.activeTimers().length === 0, `${page.liveCalls().length - n} call(s)`);
  page.done();
}

// ---- 7: polling stops while hidden and on navigation away ----
{
  const page = await loadLive(`?match=${MATCH_ID}`, answering(fixture()));
  page.fireVisibility("hidden");
  check("hidden stops the timer", page.activeTimers().length === 0);
  const n = page.liveCalls().length;
  await page.settle();
  check("no call while hidden", page.liveCalls().length === n);
  page.fireVisibility("visible");
  await page.settle();
  check("visible again polls at once and restarts the timer", page.liveCalls().length === n + 1 && page.activeTimers().length === 1);
  page.firePagehide();
  check("navigating away stops the timer", page.activeTimers().length === 0);
  page.done();
}

// ---- 8: an invalid ?match= ----
for (const search of ["?match=abc", `?match=${MATCH_ID.toUpperCase()}`, "", `?match=${MATCH_ID}/../x`]) {
  const page = await loadLive(search, answering(fixture()));
  check(`an invalid match (${JSON.stringify(search)}) shows the not-found line`, text(page.screen).includes("That game couldn't be found."), text(page.screen).slice(0, 200));
  check(`an invalid match (${JSON.stringify(search)}) never asks for a snapshot`, page.liveCalls().length === 0 && page.activeTimers().length === 0);
  page.done();
}

// ---- 9: an unknown match (null snapshot) and a failing call ----
{
  const page = await loadLive(`?match=${MATCH_ID}`, answering(null));
  check("an unknown match shows the not-found line", text(page.screen).includes("That game couldn't be found."));
  check("an unknown match stops polling", page.activeTimers().length === 0);
  page.done();
}
{
  const MSG = "Live games couldn't be loaded right now.";
  let fail = true;
  const page = await loadLive(`?match=${MATCH_ID}`, async (cmd) => {
    if (cmd === "settings_paths") return {};
    if (cmd === "live_match") { if (fail) throw new Error(MSG); return fixture(); }
    throw new Error(`no fake answer for ${cmd}`);
  });
  check("a failing call shows the player line", text(page.screen).includes(MSG), text(page.screen).slice(0, 200));
  check("a failing call keeps retrying", page.activeTimers().length === 1);
  fail = false;
  await page.fireTimers();
  check("the next good answer draws the match", text(page.screen).includes("Giants") && !text(page.screen).includes(MSG));
  page.done();
}

console.log(fails === 0
  ? "\nPASS: the match view draws the real two-game snapshot's header, picker, plays, box score and drives; disputed plays stay unhighlighted; no-sides snapshots are side-labelled; untrusted text stays literal; polling is visible-only and stops when final; invalid ids make no call."
  : `\nFAIL: ${fails}`);
process.exit(fails ? 1 : 0);
