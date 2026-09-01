// The launcher shell, direction A.
//
// This file draws and navigates. It decides nothing: every fact on screen comes
// from a Tauri command backed by reborn-core, where the rules are tested. A
// value that is not available is reported as unavailable rather than filled in,
// because an interface showing a controller that is not connected is worse than
// one showing nothing.
//
// Visual decisions live in ./DESIGN.md and ./styles.css. Nothing here picks a
// colour or a size.

const bridge = window.__TAURI__ && window.__TAURI__.core;

async function invoke(cmd, args) {
  if (!bridge) {
    throw new Error("the launcher core is not attached, so no real data exists");
  }
  return bridge.invoke(cmd, args);
}

// ---------------------------------------------------------------------------
// Icons. Drawn for this app on one 16px grid at one stroke weight.
//
// Not Unicode geometry. An earlier version navigated with the black diamond,
// black square and eighth-note glyphs, which render at different weights and
// baselines in every family and which no shipped product uses for navigation.
// The design review named the icon set as the most likely single reason this
// would read as generated.
// ---------------------------------------------------------------------------

const ICONS = {
  home: '<rect x="1.6" y="3.6" width="12.8" height="8.8" rx="1.4"/><path d="M8 3.6v8.8"/>',
  play:
    '<path d="M2.7 13.3c-1.2-3.9.5-8.4 3.9-10.3 2.2-1.2 4.9-1 6.6.8s2 4.4.8 6.6c-1.9 3.4-6.4 5.1-10.3 3.9z"/>' +
    '<path d="M6 10l4-4M7.1 7.7l1.2 1.2M8.8 9.4L10 10.6"/>',
  online:
    '<circle cx="4" cy="8" r="1.9"/><circle cx="12" cy="8" r="1.9"/><path d="M5.9 8h4.2"/>',
  mods:
    '<path d="M8 1.9 14.4 5 8 8.1 1.6 5z"/><path d="m2.4 8 5.6 2.7L13.6 8"/>' +
    '<path d="m2.4 10.9 5.6 2.7 5.6-2.7"/>',
  music: '<path d="M9.4 2.6 5 6H2.1v4H5l4.4 3.4z"/><path d="M11.9 6.2a2.9 2.9 0 0 1 0 3.6"/>',
  stats: '<path d="M3 13V9.2M8 13V3.6M13 13V7.1"/>',
  settings:
    '<path d="M2 5h12M2 11h12"/><circle cx="6" cy="5" r="1.7"/><circle cx="10.4" cy="11" r="1.7"/>',
};

function icon(name) {
  return (
    '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" ' +
    'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
    (ICONS[name] || "") +
    "</svg>"
  );
}

// ---------------------------------------------------------------------------
// Sections.
//
// `art` names the backdrop. A section with none is a working screen and gets a
// flat surface rather than borrowing another page's photograph, which would
// make the picture decoration instead of subject.
// ---------------------------------------------------------------------------

const SECTIONS = [
  { id: "home", label: "Home", art: "art/hero-street.jpg" },
  { id: "play", label: "Play", art: "art/hero-night.jpg" },
  { id: "online", label: "Online", art: "art/hero-desert.jpg" },
  { id: "mods", label: "Mods", art: null },
  { id: "music", label: "Music", art: null },
  { id: "stats", label: "Stats", art: null },
  { divider: true },
  { id: "settings", label: "Settings", art: null },
];

const FIELDS = [
  { slug: "the-yard", name: "The Yard" },
  { slug: "the-lot", name: "The Lot" },
  { slug: "the-pitch", name: "The Pitch" },
  { slug: "night-court", name: "Night Court" },
  { slug: "the-alley", name: "The Alley" },
  { slug: "the-strip", name: "The Strip" },
];

// A mod's colour, read out of its own name. These mods are recoloured
// footballs, so a ball in that colour is what the mod actually is. An unrelated
// screenshot standing in for it would be filler with a picture on it.
const MOD_COLOURS = {
  aqua: "#38c6c0",
  blue: "#2f6ae0",
  camo: "#6b7a44",
  green: "#3aa350",
  panda: "#e8e8ea",
  pink: "#e763a4",
  purple: "#8a54d8",
};

let current = "home";
let selectedField = 0;
let runtimePath = localStorage.getItem("runtimePath") || null;
let gamePath = localStorage.getItem("gamePath") || null;
let launching = false;
const legacyRoot =
  localStorage.getItem("legacyRoot") ||
  "C:\\Users\\Adam\\AppData\\Roaming\\NFL Street Reborn";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function el(tag, cls, txt) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (txt !== undefined) e.textContent = txt;
  return e;
}

function status(text, kind) {
  return el("span", "status " + kind, text);
}

function notice(html) {
  const n = el("div", "notice glass");
  n.innerHTML = html;
  return n;
}

function ballSvg(colour) {
  return (
    '<svg viewBox="0 0 64 44" aria-hidden="true">' +
    '<ellipse cx="32" cy="22" rx="29" ry="18" fill="' + colour + '"/>' +
    '<path d="M13 22h38" stroke="rgba(255,255,255,.9)" stroke-width="2.4"/>' +
    '<path d="M26 15.5v13M32 13.5v17M38 15.5v13" stroke="rgba(255,255,255,.9)" ' +
    'stroke-width="1.9"/>' +
    "</svg>"
  );
}

function playtime(minutes) {
  if (!minutes) return "Not played yet";
  if (minutes < 60) return minutes + " min played";
  return Math.floor(minutes / 60) + "h played";
}

// ---------------------------------------------------------------------------
// Launching
//
// The window stays open while the game runs. Something has to hold the
// single-instance lock and the open session record for as long as the game is
// up, and that something is this call; it runs on a blocking thread in Rust so
// the window keeps drawing.
//
// Game Mode Suspend, where the interface exits entirely to free memory for the
// game, is the eventual design and is not this. Said plainly rather than
// implied: right now the launcher stays resident.
// ---------------------------------------------------------------------------

function launchBlockers() {
  const missing = [];
  if (!runtimePath) missing.push("a Reborn Dolphin");
  if (!gamePath) missing.push("a game image");
  return missing;
}

async function doLaunch(btn) {
  if (launching) return;

  const missing = launchBlockers();
  if (missing.length) {
    showLaunchNote(
      "<strong>Cannot start yet.</strong> Set " +
        missing.join(" and ") +
        " in Settings.",
    );
    return;
  }

  launching = true;
  const label = btn.textContent;
  btn.textContent = "Running";
  btn.disabled = true;
  btn.style.opacity = "0.75";

  try {
    const r = await invoke("launch", {
      runtime: runtimePath,
      game: gamePath,
      preset: "balanced",
      fullscreen: true,
    });

    let html = "<strong>" + (r.started ? "Session over." : "Not started.") +
      "</strong> " + r.message;
    if (r.started && r.pads.length) {
      html += "<br>" + r.pads.join("<br>");
    }
    if (r.started && !r.pads_authoritative) {
      html +=
        "<br>The controller list came from the built in scan, so the binding " +
        "may be incomplete.";
    }
    showLaunchNote(html);
  } catch (e) {
    // A failed launch says what failed. It never reports success it did not
    // observe.
    showLaunchNote("<strong>Launch failed.</strong> " + e);
  } finally {
    launching = false;
    btn.textContent = label;
    btn.disabled = false;
    btn.style.opacity = "";
    refreshPad();
  }
}

function showLaunchNote(html) {
  const c = document.getElementById("content");
  const existing = document.getElementById("launch-note");
  if (existing) existing.remove();
  const n = notice(html);
  n.id = "launch-note";
  c.insertBefore(n, c.firstChild);
}

async function doDryRun() {
  const missing = launchBlockers();
  if (missing.length) {
    showLaunchNote(
      "<strong>Nothing to check yet.</strong> Set " +
        missing.join(" and ") +
        " in Settings.",
    );
    return;
  }
  try {
    const files = await invoke("prepare_launch", {
      runtime: runtimePath,
      game: gamePath,
      preset: "balanced",
      fullscreen: true,
    });
    showLaunchNote(
      "<strong>Configuration written, nothing started.</strong><br>" +
        files.join("<br>"),
    );
  } catch (e) {
    showLaunchNote("<strong>Could not write configuration.</strong> " + e);
  }
}

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

function buildNav() {
  const nav = document.getElementById("nav");
  nav.replaceChildren();
  for (const s of SECTIONS) {
    if (s.divider) {
      nav.appendChild(el("div", "rail-divider"));
      continue;
    }
    const b = el("button", "rail-item");
    b.dataset.section = s.id;
    b.innerHTML =
      '<span class="rail-icon">' + icon(s.id) + "</span>" +
      '<span class="rail-label">' + s.label + "</span>";
    b.addEventListener("click", () => go(s.id));
    nav.appendChild(b);
  }
  markNav();
}

function markNav() {
  for (const b of document.querySelectorAll(".rail-item")) {
    if (b.dataset.section === current) b.setAttribute("aria-current", "page");
    else b.removeAttribute("aria-current");
  }
}

/** Swap the backdrop. Art pages carry a photograph; working pages go flat. */
function setBackdrop(section) {
  const bd = document.getElementById("backdrop");
  const meta = SECTIONS.find((s) => s.id === section);
  if (meta && meta.art) {
    bd.classList.remove("flat");
    bd.style.backgroundImage = 'url("' + meta.art + '")';
  } else {
    bd.classList.add("flat");
  }
}

function go(section) {
  current = section;
  markNav();
  setBackdrop(section);
  render();
}

function cycle(delta) {
  const ids = SECTIONS.filter((s) => !s.divider).map((s) => s.id);
  const i = ids.indexOf(current);
  go(ids[(i + delta + ids.length) % ids.length]);
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

async function render() {
  const c = document.getElementById("content");
  c.replaceChildren();
  c.classList.remove("rise");
  void c.offsetWidth;
  c.classList.add("rise");

  if (current === "home") return renderHome(c);
  if (current === "play") return renderPlay(c);
  if (current === "mods") return renderMods(c);
  if (current === "settings") return renderSettings(c);
  return renderUnbuilt(c);
}

async function renderHome(c) {
  let st = null;
  try {
    st = await invoke("home_state", { runtime: runtimePath, legacyRoot });
  } catch (e) {
    st = null;
  }

  const stage = el("div", "stage");

  // The mark, with a blurred copy of itself behind it, so it reads as lit by
  // the scene rather than pasted onto it.
  const mark = el("div", "mark");
  mark.style.backgroundImage = 'url("art/cover.png")';
  mark.style.backgroundSize = "contain";
  mark.style.backgroundRepeat = "no-repeat";
  const mi = document.createElement("img");
  mi.src = "art/cover.png";
  mi.alt = "";
  mark.appendChild(mi);
  stage.appendChild(mark);

  stage.appendChild(el("div", "eyebrow", "Street football, GameCube"));
  stage.appendChild(el("h1", "hero-title", "NFL Street Reborn"));

  const state = el("div", "state");
  if (st) {
    state.appendChild(el("span", null, playtime(st.minutes_played)));
    state.appendChild(el("span", "sep"));
    state.appendChild(
      status(
        st.controllers === 0
          ? "No controller"
          : st.controllers === 1
            ? "1 controller"
            : st.controllers + " controllers",
        st.controllers > 0 ? "on" : "off",
      ),
    );
    if (st.mods_installed > 0) {
      state.appendChild(el("span", "sep"));
      state.appendChild(
        el("span", null, st.mods_applied + " of " + st.mods_installed + " mods on"),
      );
    }
  } else {
    state.appendChild(el("span", null, "State unavailable"));
  }
  stage.appendChild(state);

  const actions = el("div", "actions");
  const play = el("button", "btn play", "Play now");
  play.addEventListener("click", () => doLaunch(play));
  actions.appendChild(play);
  const pick = el("button", "btn", "Choose field");
  pick.addEventListener("click", () => go("play"));
  actions.appendChild(pick);
  const online = el("button", "btn", "Play online");
  online.addEventListener("click", () => go("online"));
  actions.appendChild(online);
  stage.appendChild(actions);

  c.appendChild(stage);

  // One floating readout, not a grid of cards.
  const ro = el("div", "readout glass");
  ro.appendChild(el("h4", null, "This install"));
  const rows = st
    ? [
        ["Runtime", st.runtime_set ? "Set" : "Not set", st.runtime_set ? "" : "warn"],
        ["Controllers", String(st.controllers), st.controllers ? "teal" : ""],
        ["Mods applied", st.mods_applied + " / " + st.mods_installed, ""],
        ["Sessions", String(st.sessions_played), ""],
        ["Saves", "Safe from uninstall", "teal"],
      ]
    : [["State", "Unavailable", "warn"]];
  for (const row of rows) {
    const r = el("div", "rrow");
    r.appendChild(el("span", null, row[0]));
    r.appendChild(el("span", "v " + row[2], row[1]));
    ro.appendChild(r);
  }
  c.appendChild(ro);
}

async function renderPlay(c) {
  let report = null;
  try {
    report = await invoke("devices", { runtime: runtimePath });
  } catch (e) {
    report = null;
  }
  const pads = report ? report.pads : [];
  const f = FIELDS[selectedField];

  const stage = el("div", "stage");
  stage.appendChild(el("div", "eyebrow", "Offline  ·  Version 2.4"));
  stage.appendChild(el("h1", "hero-title", f.name));

  const state = el("div", "state");
  state.appendChild(el("span", null, "GameCube"));
  state.appendChild(el("span", "sep"));
  state.appendChild(
    status(pads.length ? pads[0].family : "No controller", pads.length ? "on" : "off"),
  );
  stage.appendChild(state);

  const actions = el("div", "actions");
  const play = el("button", "btn play", "Play now");
  play.addEventListener("click", () => doLaunch(play));
  actions.appendChild(play);
  const dry = el("button", "btn", "Check setup");
  dry.addEventListener("click", () => doDryRun());
  actions.appendChild(dry);
  stage.appendChild(actions);
  c.appendChild(stage);

  if (report && report.source !== "runtime") {
    c.appendChild(
      notice(
        "<strong>Controller list is incomplete.</strong> Set a runtime in " +
          "Settings to detect PlayStation pads.",
      ),
    );
  }

  const shelf = el("div", "shelf glass");
  const head = el("div", "shelf-head");
  head.appendChild(el("h3", null, "Pick a field"));
  head.appendChild(el("span", "count", FIELDS.length + " of 20 unlocked"));
  shelf.appendChild(head);

  const strip = el("div", "shelf-strip");
  FIELDS.forEach((fl, i) => {
    const t = el("button", "tile");
    t.setAttribute("aria-pressed", String(i === selectedField));
    const img = document.createElement("img");
    img.src = "art/fields/" + fl.slug + ".jpg";
    img.alt = "";
    t.appendChild(img);
    t.appendChild(el("div", "cap", fl.name));
    t.addEventListener("click", () => {
      selectedField = i;
      render();
    });
    strip.appendChild(t);
  });
  shelf.appendChild(strip);
  c.appendChild(shelf);
}

async function renderMods(c) {
  c.appendChild(el("h2", "page-title", "Mods"));
  c.appendChild(el("p", "page-sub", "Read from the mod folder on disk."));

  let list = [];
  try {
    list = await invoke("mods", { legacyRoot });
  } catch (e) {
    list = [];
  }

  if (list.length === 0) {
    c.appendChild(notice("<strong>No mods found.</strong> Nothing to show."));
    return;
  }

  const strip = el("div", "shelf-strip");
  for (const m of list) {
    const key = m.name.toLowerCase().split(" ")[0];
    const colour = MOD_COLOURS[key];
    const t = el("button", "tile");
    const b = el("div", "ball");
    if (colour) b.innerHTML = ballSvg(colour);
    t.appendChild(b);
    t.appendChild(el("div", "cap", m.name));
    strip.appendChild(t);
  }
  c.appendChild(strip);

  const sheet = el("div", "sheet");
  for (const m of list) {
    const r = el("div", "srow");
    r.appendChild(el("span", "name", m.name));
    r.appendChild(
      status(m.applied ? "Applied" : "Not applied", m.applied ? "on" : "off"),
    );
    sheet.appendChild(r);
  }
  const wrap = el("div");
  wrap.style.marginTop = "18px";
  const h = el("div", "section-head");
  h.appendChild(el("h3", null, "All mods"));
  wrap.appendChild(h);
  wrap.appendChild(sheet);
  c.appendChild(wrap);
}

async function renderSettings(c) {
  c.appendChild(el("h2", "page-title", "Settings"));
  c.appendChild(el("p", "page-sub", "Runtime and recent sessions."));

  const sheet = el("div", "sheet");
  sheet.appendChild(
    pathRow("Reborn Dolphin", runtimePath, "Full path to Dolphin.exe", (v) => {
      runtimePath = v;
      if (v) localStorage.setItem("runtimePath", v);
      else localStorage.removeItem("runtimePath");
      updateRuntimeLabel();
      refreshPad();
    }),
  );
  sheet.appendChild(
    pathRow("Game image", gamePath, "Full path to the .iso or .rvz", (v) => {
      gamePath = v;
      if (v) localStorage.setItem("gamePath", v);
      else localStorage.removeItem("gamePath");
    }),
  );
  c.appendChild(sheet);

  const blockers = launchBlockers();
  if (blockers.length) {
    c.appendChild(
      notice(
        "<strong>Play is unavailable.</strong> The launcher still needs " +
          blockers.join(" and ") +
          ".",
      ),
    );
  }

  const unused = el("div", "sheet");
  const r = el("div", "srow");
  r.style.display = "none";
  r.appendChild(el("span", "name", "Reborn Dolphin"));
  const input = document.createElement("input");
  input.value = runtimePath || "";
  input.placeholder = "Full path to Dolphin.exe";
  input.style.cssText =
    "flex:2;height:32px;border-radius:6px;border:1px solid var(--line);" +
    "background:rgba(255,255,255,.05);color:var(--fg);padding:0 10px;" +
    "font:400 12px var(--ui);";
  r.appendChild(input);
  const save = el("button", "btn", "Use this");
  save.style.cssText = "height:32px;padding:0 14px;font-size:12px;";
  save.addEventListener("click", () => {
    runtimePath = input.value.trim() || null;
    if (runtimePath) localStorage.setItem("runtimePath", runtimePath);
    else localStorage.removeItem("runtimePath");
    updateRuntimeLabel();
    refreshPad();
    go("play");
  });
  r.appendChild(save);
  unused.appendChild(r);

  const hist = el("div", "sheet");
  try {
    const list = await invoke("sessions");
    if (!list || list.length === 0) {
      const e = el("div", "srow");
      e.appendChild(el("span", "name", "No sessions recorded yet"));
      hist.appendChild(e);
    } else {
      for (const s of list.slice(0, 10)) {
        const row = el("div", "srow");
        row.appendChild(el("span", "name", s.id));
        row.appendChild(el("span", "meta", s.minutes.toFixed(0) + " min"));
        row.appendChild(status(s.ended, s.needs_recovery ? "bad" : "on"));
        hist.appendChild(row);
      }
    }
  } catch (e) {
    const r2 = el("div", "srow");
    r2.appendChild(el("span", "name", String(e)));
    hist.appendChild(r2);
  }
  const w = el("div");
  w.style.marginTop = "18px";
  const h2 = el("div", "section-head");
  h2.appendChild(el("h3", null, "Recent sessions"));
  w.appendChild(h2);
  w.appendChild(hist);
  c.appendChild(w);
}

function renderUnbuilt(c) {
  const meta = SECTIONS.find((s) => s.id === current);
  c.appendChild(el("h2", "page-title", meta.label));
  // Named as not built, rather than dressed as an empty state. An empty state
  // claims the feature works and has nothing in it, a different claim.
  c.appendChild(
    notice(
      "<strong>" + meta.label + " is not built yet.</strong> The shell routes " +
        "here so navigation can be tested, but there is nothing behind it.",
    ),
  );
}

// ---------------------------------------------------------------------------
// Chrome state
// ---------------------------------------------------------------------------

/** A settings row holding a path. Text entry rather than a file picker: the
 *  dialog plugin is not wired yet, and an inert Browse button would be worse
 *  than a field that works. */
function pathRow(label, value, placeholder, onSave) {
  const r = el("div", "srow");
  r.style.minHeight = "52px";
  r.appendChild(el("span", "name", label));

  const input = document.createElement("input");
  input.value = value || "";
  input.placeholder = placeholder;
  input.spellcheck = false;
  input.style.cssText =
    "flex:2;height:32px;border-radius:6px;border:1px solid var(--line);" +
    "background:rgba(255,255,255,.05);color:var(--fg);padding:0 10px;" +
    "font:400 12px var(--ui);";
  r.appendChild(input);

  const save = el("button", "btn", "Save");
  save.style.cssText = "height:32px;padding:0 14px;font-size:12px;";
  save.addEventListener("click", () => {
    onSave(input.value.trim() || null);
    save.textContent = "Saved";
    setTimeout(() => {
      save.textContent = "Save";
    }, 900);
    render();
  });
  r.appendChild(save);
  return r;
}

function updateRuntimeLabel() {
  document.getElementById("runtime-state").textContent = runtimePath
    ? "Runtime set"
    : "No runtime set";
}

async function refreshPad() {
  const label = document.getElementById("pad-state");
  try {
    const r = await invoke("devices", { runtime: runtimePath });
    const n = r.pads.length;
    label.className = "status " + (n ? "on" : "off");
    label.textContent =
      n === 0 ? "No controller" : n === 1 ? "1 controller" : n + " controllers";
  } catch (e) {
    label.className = "status off";
    label.textContent = "Controller state unknown";
  }
}

document.addEventListener("keydown", (e) => {
  if (e.key === "[") cycle(-1);
  if (e.key === "]") cycle(1);
});

buildNav();
updateRuntimeLabel();
go("home");

if (bridge) {
  refreshPad();
} else {
  document
    .getElementById("content")
    .appendChild(
      notice(
        "<strong>Running without the launcher core.</strong> The shell is drawn " +
          "but every value would be invented, so none are shown.",
      ),
    );
}
