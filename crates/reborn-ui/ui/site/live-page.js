// Vendored from Reborn Backend src/web/live-page.js @daf8b3a, do not edit: run Tools/sync-site-render.mjs.
// The launcher match page (match.js) runs it unchanged, answering its fetch and stream itself.
// The live game page's controller (/live/<match>, site restyle spec 2026-10-02). Reads the match id from
// <meta name="nsr-match">, fetches the snapshot, then follows the play stream. Everything on screen is built by
// live-render.js (createElement/textContent only); this file only wires state, events and the stream.
//
//  - Page: the scoreboard strip, then three tabs: Live Feed (the latest-play panel over All plays, a drive list with an
//    All plays / Scoring plays toggle), Box Score, Team Stats (game page v2 spec 2026-10-03).
//    Each panel has its own plain-text game picker (R12); Box Score and Team Stats default to Series total once there are two games. The strip follows the Live Feed's game.
//  - GET /v1/live/<id>, then EventSource('/v1/live/<id>/stream?since=<max rev>'). Each `play` event is upserted by
//    (game_seq, seq) and the page re-renders. The stream stopping for good closes it; the 30 s tick refetches while
//    it is not open (final review I2) and reopens it while the match runs.
//  - The snapshot carries verified box lines and drives for recorded games (R15): no extra /v1/games reads. After the
//    match the tick keeps refreshing, at most FINAL_TRIES times, while a recorded game's box is still provisional.
//  - All plays is patched, never rebuilt (live card M4): drives are keyed rows, a new play redraws only the current
//    drive, and the drive or card being read keeps its place on screen (game page v2 R15). Open drives (R5) and the
//    toggle (R1) are page state.
//  - A screen reader hears only the newest play (#latest), never the rebuilt panels.
import {
  renderStrip, renderPicker, renderPlayPanel, renderAllPlays, renderBoxScore, renderTeamStats, readingAnchor,
  newestGame, upsertPlay, letterBadge, newestPlayLine, refetchOnTick, keepRows, patchList, seqOf, panelStepSeq, panelPickSeq,
} from "./live-render.js";

const ENDED = new Set(["complete", "void", "abandoned"]);
const REFRESH_MS = 30_000;
const CLOSED_REFETCH_GAP_MS = 10_000;   // a stream that closes right after a refetch does not trigger another one
const FINAL_TRIES = 20;        // after the match: keep refreshing for verified box scores this many times, then stop
const RECENT_MAX = 500;
const TABS = ["feed", "box", "team"];

const matchId = document.querySelector('meta[name="nsr-match"]')?.getAttribute("content") ?? "";
const $ = (id) => document.getElementById(id);
const strip = $("strip"), feedPick = $("feed-pick"), panelSlot = $("panel"), allSlot = $("allplays"), note = $("note"), latest = $("latest");
const panels = { feed: $("tab-feed"), box: $("tab-box"), team: $("tab-team") };

let snap = null;
let tab = "feed";
let es = null;
let timer = null;
let finalTries = 0;
let lastRefreshDone = 0;
let recent = [];               // stream frames, re-applied over a snapshot that was read before they arrived
const picks = { feed: null, box: null, team: null };   // a game_seq, "total", or null (the default)
const open = new Map();        // "g<game_seq>:d<first_seq>" -> the viewer's open/closed choice for a drive (R5)
let apMode = "all";            // All plays or Scoring plays (R1)
let panelSeq = null;           // the play the Latest play panel shows; null follows the newest
const badImages = new Set();   // logo / portrait URLs that failed to load: drawn as their fallbacks from then on
const badAvatars = new Set();

const ended = () => (snap?.match?.outcome?.kind ? snap.match.outcome.kind !== "open" : ENDED.has(snap?.match?.status));

function maxRev(s = snap) {
  let top = 0;
  for (const g of s?.games ?? []) for (const p of g.plays ?? []) if (Number.isFinite(p.rev) && p.rev > top) top = p.rev;
  return top;
}

function el(tag, className, text) {
  const n = document.createElement(tag);
  if (className) n.className = className;
  if (text !== undefined) n.textContent = text;
  return n;
}

function setNote(text) { note.textContent = text; }

const gamesOf = () => [...(snap?.games ?? [])].sort((a, b) => a.game_seq - b.game_seq);
const gameOf = (seq) => gamesOf().find((g) => g.game_seq === seq) ?? newestGame(snap);

/** A panel's pick (R12): the viewer's when it still exists, else Series total for Box Score / Team Stats once there
 *  are two games, else the newest game. */
function pickOf(name) {
  const p = picks[name];
  const games = gamesOf();
  const totals = name === "box" || name === "team";
  if (p === "total" && totals && games.length > 1) return "total";
  if (Number.isInteger(p) && games.some((g) => g.game_seq === p)) return p;
  if (totals && games.length > 1) return "total";
  return newestGame(snap)?.game_seq ?? null;
}

const picker = (name) => renderPicker(gamesOf(), pickOf(name), document, { total: name === "box" || name === "team", name });

/** An All plays row's identity: its key, classes, words and images. Equal -> the row on screen stays. */
const rowSig = (li) => `${li.dataset.key ?? li.dataset.seq}
${li.className}
${li.textContent}
${[...li.querySelectorAll("img")].map((i) => i.getAttribute("src")).join(" ")}`;

/** All plays is patched, not rebuilt (M4): drives are keyed rows; only a changed drive is replaced, and the drive or
 *  card being read keeps its place (R15). A new game or a new toggle mode is drawn afresh. */
function drawAllPlays(game) {
  const next = renderAllPlays(snap, game, document, { badImages, open, mode: apMode });
  const fresh = next.querySelector("ol.ap");
  const old = allSlot.querySelector("ol.ap");
  if (!old || !fresh || old.dataset.game !== fresh.dataset.game || old.dataset.mode !== fresh.dataset.mode) { allSlot.replaceChildren(next); return; }
  // The page scrolls (no inner scroller): hold the innermost visible drive or card still where it is, unless at the
  // very top. It is found again by its key, because a changed drive is a new node.
  const anchor = window.scrollY > 0 ? readingAnchor(old) : null;
  const key = anchor?.dataset.key ?? null;
  const before = anchor ? anchor.getBoundingClientRect().top : 0;
  // The anchor's drive row, for when a possession change collapses the current drive and the card's key is gone.
  const driveRow = anchor ? (anchor.matches("li.dv") ? anchor : anchor.closest("li.dv")) : null;
  const driveKey = driveRow?.dataset.key ?? null;
  const driveBefore = driveRow ? driveRow.getBoundingClientRect().top : 0;
  // A changed drive is replaced whole, so keyboard focus inside it is put back on the new node afterwards.
  const held = document.activeElement;
  const heldSel = held && old.contains(held)
    ? (held.matches(".dv-hd") ? `.dv-hd[data-drive="${held.dataset.drive}"]` : held.matches(".ap-btn") ? `.ap-btn[data-seq="${held.dataset.seq}"]` : null)
    : null;
  patchList(old, keepRows([...old.children], [...fresh.children], rowSig));
  const again = key ? old.querySelector(`[data-key="${key}"]`) : null;
  if (again) window.scrollBy(0, again.getBoundingClientRect().top - before);
  else if (driveKey) {
    const driveAgain = old.querySelector(`li.dv[data-key="${driveKey}"]`);
    if (driveAgain) window.scrollBy(0, driveAgain.getBoundingClientRect().top - driveBefore);
  }
  if (heldSel && !held.isConnected) old.querySelector(heldSel)?.focus({ preventScroll: true });
}

let queued = false;
function render() {
  if (queued) return;
  queued = true;
  requestAnimationFrame(() => { queued = false; draw(); });
}

/** Which picker button had focus, so a redraw can put focus back on its replacement. */
function focusedPick() {
  const a = document.activeElement;
  const nav = a?.closest?.(".picker");
  return nav && a.dataset.pick ? `.picker[data-for="${nav.dataset.for}"] button[data-pick="${a.dataset.pick}"]` : null;
}

function draw() {
  if (!snap) return;
  const had = focusedPick();
  const game = gameOf(pickOf("feed"));
  strip.replaceChildren(renderStrip(snap, game, document, { badImages, badAvatars }));
  const line = newestPlayLine(snap);
  if (latest.textContent !== line) latest.textContent = line;
  if (!game) {
    const msg = snap.match?.status === "running" ? "Waiting for the first play." : "No plays were recorded for this match.";
    panelSlot.replaceChildren(el("p", "empty", msg));
    allSlot.replaceChildren();
    for (const k of ["box", "team"]) panels[k].replaceChildren(el("p", "empty", msg));
    return;
  }
  feedPick.replaceChildren(...[picker("feed")].filter(Boolean));
  panelSlot.replaceChildren(renderPlayPanel(snap, game, document, { seq: panelSeq, badImages }));
  drawAllPlays(game);
  panels.box.replaceChildren(...[picker("box"), renderBoxScore(snap, pickOf("box"), document, { badImages })].filter(Boolean));
  panels.team.replaceChildren(...[picker("team"), renderTeamStats(snap, pickOf("team"), document, { badImages })].filter(Boolean));
  if (had) document.querySelector(had)?.focus();
}

/** « ◀ ▶ Live: step the panel through the picked game's plays; reaching the newest follows it again. */
function stepPanel(how) {
  const game = gameOf(pickOf("feed"));
  if (!game?.plays?.length) return;
  panelSeq = panelStepSeq(game, panelSeq, how);
  draw();
  const again = panelSlot.querySelector(`.pp-step[data-step="${how}"]`);
  (again && !again.disabled ? again : panelSlot.querySelector('.pp-step[data-step="live"]'))?.focus();
}

// One delegated listener: game pickers, the panel's arrows, the All plays toggle, drive rows (open/close) and play
// cards (a card shows its play in the panel).
document.addEventListener("click", (e) => {
  const t = e.target instanceof Element ? e.target : null;
  const pick = t?.closest(".picker button[data-pick]");
  if (pick) {
    const name = pick.closest(".picker").dataset.for;
    picks[name] = pick.dataset.pick === "total" ? "total" : Number(pick.dataset.pick);
    if (name === "feed") panelSeq = null;
    draw();
    return;
  }
  const step = t?.closest(".pp-step");
  if (step) { if (!step.disabled) stepPanel(step.dataset.step); return; }
  const mode = t?.closest(".ap-seg button[data-mode]");
  if (mode) {
    apMode = mode.dataset.mode === "scoring" ? "scoring" : "all";
    draw();
    allSlot.querySelector(`.ap-seg button[data-mode="${apMode}"]`)?.focus();
    return;
  }
  const drive = t?.closest(".dv-hd");
  if (drive) {
    const k = drive.dataset.drive;
    open.set(k, drive.getAttribute("aria-expanded") !== "true");
    draw();
    allSlot.querySelector(`.dv-hd[data-drive="${k}"]`)?.focus();
    return;
  }
  const row = t?.closest(".ap-btn");
  if (row) {
    panelSeq = panelPickSeq(gameOf(pickOf("feed")), Number(row.dataset.seq));
    draw();
    panelSlot.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }
});

// An image that fails to load: an avatar becomes the letter badge; a logo or portrait is remembered and the page
// redrawn, so every renderer draws its fallback (a logo's abbrev badge, a portrait's team logo) from then on.
// `error` does not bubble: listen in the capture phase.
document.addEventListener("error", (e) => {
  const img = e.target;
  if (!(img instanceof HTMLImageElement)) return;
  const src = img.getAttribute("src");
  if (img.classList.contains("avatar")) {
    badAvatars.add(src);
    img.replaceWith(letterBadge(document, img.dataset.initial));
    return;
  }
  badImages.add(src);
  img.remove();
  render();
}, true);

// ---- tabs: the ARIA tab pattern (roving tabindex; Left/Right/Home/End move and select) ----
const tabButtons = () => [...document.querySelectorAll('.tabs [role="tab"]')];

function showTab(name, { focus = false } = {}) {
  tab = name;
  for (const b of tabButtons()) {
    const on = b.dataset.tab === name;
    b.setAttribute("aria-selected", String(on));
    b.tabIndex = on ? 0 : -1;
    if (on && focus) b.focus();
  }
  for (const [k, p] of Object.entries(panels)) p.hidden = k !== name;
  // Box score and team stats come from the snapshot, which the tick skips while the stream is open: opening
  // one after new plays arrived refetches it (the viewer asked; unchanged is a cheap 304).
  if (recent.length && snap && name !== "feed") refresh();
}

for (const b of tabButtons()) {
  b.addEventListener("click", () => showTab(b.dataset.tab));
  b.addEventListener("keydown", (e) => {
    const i = TABS.indexOf(tab);
    const to = { "ArrowRight": (i + 1) % TABS.length, "ArrowLeft": (i + TABS.length - 1) % TABS.length, "Home": 0, "End": TABS.length - 1 }[e.key];
    if (to === undefined) return;
    e.preventDefault();
    showTab(TABS[to], { focus: true });
  });
}

/** Upsert one stream frame by (game_seq, seq); the game is rescored from its whole play list (out-of-order safe). */
const upsert = (frame, rev) => upsertPlay(snap, frame, rev);

function onPlay(e) {
  let frame;
  try { frame = JSON.parse(e.data); } catch { return; }
  const rev = Number(e.lastEventId);
  recent.push({ frame, rev });
  if (recent.length > RECENT_MAX) recent = recent.slice(-RECENT_MAX);
  if (upsert(frame, rev)) refresh();
  render();
}

function openStream() {
  if (es || !snap || ended()) return;
  es = new EventSource(`/v1/live/${encodeURIComponent(matchId)}/stream?since=${maxRev()}`);
  es.addEventListener("play", onPlay);
  es.addEventListener("open", () => setNote(""));   // the strip already says LIVE (R12)
  es.addEventListener("error", () => {
    // CONNECTING: the browser retries on its own. CLOSED: a 204 (over and caught up) or a refused open -- stop.
    if (es && es.readyState === EventSource.CLOSED) {
      es.close();
      es = null;
      setNote("Updating every 30 s");
      // Refetch (it shows Final when the match is over) but do not reopen here: a refused open would loop. The
      // 30-second tick reopens the stream while the match is still running.
      if (Date.now() - lastRefreshDone >= CLOSED_REFETCH_GAP_MS) refresh({ reopen: false });
    } else setNote("Reconnecting…");
  });
}

function closeStream() { if (es) { es.close(); es = null; } }

let inflight = null, again = false;
async function refresh({ reopen = true } = {}) {
  if (inflight) { again = again || reopen; return inflight; }
  inflight = (async () => {
    try {
      const res = await fetch(`/v1/live/${encodeURIComponent(matchId)}`, { headers: { accept: "application/json" } });
      if (res.status === 404) {
        closeStream(); stopTimer();
        strip.replaceChildren(el("p", "empty", "No such match."));
        return;
      }
      if (!res.ok) { setNote("Could not refresh; trying again shortly."); return; }
      const body = await res.json();
      const cut = maxRev(body);
      snap = body;
      for (const { frame, rev } of recent) if (rev > cut) upsert(frame, rev);
      recent = recent.filter((x) => x.rev > cut);
      render();
      if (ended()) {
        closeStream();
        setNote("");   // the strip already says Final (R12)
        const waiting = (snap.games ?? []).some((g) => g.game_id && g.box?.provisional !== false);
        if (!waiting || ++finalTries > FINAL_TRIES) stopTimer();
      } else if (reopen) openStream();
    } catch {
      setNote("Could not refresh; trying again shortly.");
    } finally {
      lastRefreshDone = Date.now();
    }
  })();
  try { await inflight; } finally {
    inflight = null;
    if (again) { again = false; refresh(); }
  }
}

function stopTimer() { if (timer) { clearInterval(timer); timer = null; } }

showTab(tab);
refresh();
timer = setInterval(() => { if (refetchOnTick(es)) refresh(); }, REFRESH_MS);
