// Play, as a page.
//
// One button and the facts behind it. Everything the button will use is read
// first and shown in words, so a launch is never a surprise, and everything
// the launch said afterwards is shown the same way.

import { bridge, call, el, fact, facts, get, set, log, markCurrent, minutes, partial, reason, refusal, wireCoreFold } from "./shell.js";

// Each read settles on its own: a failed pad scan must not blank the page.
const state = {};
let busy = false;
let playing = false;
let result = null;
let problem = null;
let detailsOpen = false;
let externalSession = false;

const PRESETS = [
  ["performance", "Performance", "1080p · 3×. ReShade disabled."],
  ["balanced", "Balanced", "1080p · 3×. ReShade with lighter effects."],
  ["quality", "All-Out", "1080p · 3×. ReShade with high-end effects."],
];

function graphicsLabel() { return state.graphicsOptions?.current?.label || PRESETS.find(p=>p[0]===preset())?.[1] || "Balanced"; }

function preset() {
  const chosen = get("preset");
  if (chosen) return chosen === "ultra" ? "quality" : chosen;
  const s = state.progress && state.progress.summary;
  return s?.graphics === "ultra" ? "quality" : (s && s.graphics) || "balanced";
}

async function load() {
  const reads = {
    active: call("session_active"),
    graphicsOptions: call("setup_graphics_options"),
    check: call("launch_check", { runtime: get("runtime"), game: get("game"), userDir: get("userDir") }),
    devices: call("devices", { runtime: get("runtime") }),
    mods: call("mod_packages"),
    controls: call("controls_current"),
    home: call("home_state", { runtime: get("runtime"), legacyRoot: get("legacyRoot") }),
    progress: call("onboarding_progress", { legacyRoot: get("legacyRoot") }),
    paths: call("paths"),
  };
  const keys = Object.keys(reads);
  const settled = await Promise.allSettled(Object.values(reads));
  keys.forEach((k, i) => {
    state[k] = settled[i].status === "fulfilled" ? settled[i].value : { error: reason(settled[i].reason) };
  });
}

async function play() {
  if (busy || externalSession) return;
  busy = true;
  playing = true;
  result = null;
  problem = null;
  draw();
  try {
    result = await call("launch", {
      runtime: get("runtime"),
      game: get("game"),
      preset: preset(),
      fullscreen: true,
      userDir: get("userDir"),
      // Required by the command, and its absence made Play fail outright with
      // "command launch missing required key modsDisabled". A per-launch
      // override only: false means launch with whatever mods are active, which
      // is what Play has always meant. Turning mods off lives on the mods page
      // and in recovery, not here.
      modsDisabled: false,
    });
    if (result && result.started === false) { problem = result.message || "The game could not start."; result = null; }
  } catch (e) {
    problem = reason(e);
  }
  playing = false;
  busy = false;
  await load();
  draw();
}

function ok(x) {
  return x && !x.error;
}

async function openDolphin() {
  if (busy || externalSession) return;
  busy = true;
  problem = null;
  draw();
  try {
    const r = await call("open_runtime", { runtime: get("runtime"), userDir: get("userDir") });
    result = { message: `Opened ${r.runtime}.`, user_dir_note: r.user_dir_note };
  } catch (e) {
    problem = reason(e);
  }
  busy = false;
  draw();
}

function when(ms) {
  if (typeof ms !== "number") return "unknown";
  try {
    return new Date(ms).toLocaleString();
  } catch (_) {
    return "unknown";
  }
}

function padsLine() {
  const d = state.devices;
  if (!ok(d)) return { text: null, note: d && d.error };
  const pads = Array.isArray(d.pads) ? d.pads : [];
  if (pads.length === 0) {
    return {
      text: "none found",
      note: d.complete ? "A sleeping wireless pad counts as none. Wake it and reload." : `Only some backends were searched (${(d.unsearched || []).join(", ") || "unknown"}), so this may be incomplete.`,
    };
  }
  const names = pads.map((p) => (p.port ? `port ${p.port}: ` : "") + p.device_string).join("; ");
  return { text: `${pads.length} found. ${names}`, note: d.complete ? null : "Not every backend was searched." };
}

function modsLine() {
  const m = state.mods;
  if (!ok(m)) return { text: null, note: m && m.error };
  const on = Array.isArray(m.activation) ? m.activation : [];
  if (on.length === 0) return { text: "none on", note: null };
  const named = on.map((a) => {
    const p = (m.packages || []).find((x) => x.id === a.id);
    return p ? p.name : `${a.id} (not installed)`;
  });
  return { text: `${on.length} on: ${named.join(", ")}`, note: null };
}

function graphicsBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Graphics" }));
  sec.append(el("p", { class: "note", text: "Applied when you next start the game." }));
  const current = state.graphicsOptions?.current ? null : preset();
  if(state.graphicsOptions?.current) sec.append(el("p",{class:"note",text:`${state.graphicsOptions.current.label} � ${state.graphicsOptions.current.resolution}� resolution. Selecting a preset replaces these settings.`}));
  const grid = el("div", { class: "preset-grid" });
  for (const [slug, label, why] of PRESETS) {
    const input = el("input", {
      type: "radio",
      name: "preset",
      checked: slug === current,
      value: slug,
      disabled: busy || externalSession,
      onchange: async () => {
        try { await call("setup_save_graphics_options",{mode:"preset",resolution:3,msaa:1,ssaa:false}); state.graphicsOptions={current:null}; } catch(e) { problem=reason(e);draw();return; }
        set("preset", slug);
        // The native radio has already changed. Keep this panel mounted so
        // changing resolution does not replay its entrance or reset scrolling.
        for (const radio of document.querySelectorAll('input[name="preset"]')) {
          radio.closest('.choice')?.classList.toggle('selected', radio.checked);
        }
        const label = document.getElementById('graphics-preset-name');
        if (label) label.textContent = PRESETS.find(p => p[0] === slug)?.[1] || slug;
        for (const row of document.querySelectorAll('.facts tr')) {
          if (row.querySelector('th')?.textContent === 'Graphics') row.querySelector('td').textContent = PRESETS.find(p => p[0] === slug)?.[1] || slug;
        }
      },
    });
    grid.append(el("label", { class: "choice" + (slug === current ? " selected" : "") }, input, el("span", { class: "what", text: label }), el("span", { class: "why", text: why })));
  }
  sec.append(grid);
  return sec;
}

function factsBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "What Play will use" }));
  const check = state.check;
  const pads = padsLine();
  const mods = modsLine();
  const c = state.controls;
  const home = state.home;
  const paths = state.paths;

  const rows = [];
  rows.push(fact("Emulator", ok(check) ? check.runtime : null));
  if (ok(check) && check.runtime_note) rows.push(fact("", check.runtime_note));
  rows.push(fact("Game image", ok(check) ? check.game : null));
  if (ok(check) && check.game_note) rows.push(fact("", check.game_note));
  rows.push(fact("User folder", ok(check) ? check.user_dir_note : null));
  rows.push(fact("Controller", pads.text));
  if (pads.note) rows.push(fact("", pads.note));
  rows.push(fact("Mods", mods.text));
  if (mods.note) rows.push(fact("", mods.note));
  rows.push(fact("Button art", ok(c) && c.label ? `${c.label}, from ${c.source === "settings" ? "Settings" : "first run"}` : ok(c) ? "nothing chosen; the disc's own art" : null));
  rows.push(fact("Graphics", graphicsLabel()));
  rows.push(fact("Channel", ok(paths) ? paths.channel : null));
  if (ok(home)) {
    rows.push(
      fact(
        "Played",
        home.sessions_played
          ? `${home.sessions_played} session(s), ${minutes(home.minutes_played)}, last ${when(home.last_played_ms)}`
          : "never",
      ),
    );
  } else {
    rows.push(fact("Played", null));
  }
  sec.append(facts(rows));
  return sec;
}

function heroBlock() {
  const ready = ok(state.check) && state.check.ready;
  const inSession = playing || externalSession;
  const hero = el("section", { class: "hero", "aria-label": "Play NFL Street Reborn" });
  hero.append(el("div", { class: "hero-art", "aria-hidden": "true" }, el("img", { src: "art/court-night.png", alt: "" })));
  const content = el("div", { class: "hero-content" });
  content.append(el("h1", { class: "game-wordmark" }, el("img", { src: "art/reborn-wordmark.png", alt: "NFL Street Reborn" })));
  const home = state.home;
  const history = ok(home) && home.sessions_played ? `${minutes(home.minutes_played)} played` : "Ready for your first session";
  content.append(el("p", { class: "game-meta", text: history }));
  const actions = el("div", { class: "hero-actions" });
  if (ready || inSession) {
    actions.append(el("button", { type: "button", id: "play-now", "data-primary": "true", class: "play", text: inSession ? "Session active" : busy ? "Opening..." : "\u25b6  Play now", disabled: busy || inSession, onclick: play }));
  } else {
    actions.append(el("a", { class: "button primary play", "data-primary": "true", href: "onboarding.html", text: "Set up your game" }));
  }
  actions.append(el("a", { href: "#game-options", class: "button quiet", text: "Game options", onclick: (event) => { event.preventDefault(); detailsOpen = true; const fold = document.getElementById("game-options"); if (fold) fold.open = true; } }));
  content.append(actions);
  const status = inSession ? "Starting or running. Your session is being tracked." : ready ? "Ready to play" : "Finish setup or choose your game in Settings.";
  content.append(el("p", { class: "status-line", role: "status" }, el("span", { class: "status-dot" + (inSession ? " wait" : ready ? "" : " warn"), "aria-hidden": "true" }), status));
  hero.append(content);
  return hero;
}

function quickLink(href, label, value, note) {
  return el("a", { href, class: "quick-link" }, el("span", { class: "eyebrow", text: label }), el("strong", { text: value, id: label === "GRAPHICS" ? "graphics-preset-name" : null }), el("span", { class: "arrow", text: "\u2197", "aria-hidden": "true" }));
}

function draw() {
  if (typeof state.active === "boolean") externalSession = state.active;
  const screen = document.getElementById("screen");
  const content = el("div", { class: "home-content" });
  if (problem) content.append(refusal(problem));
  if (state.check?.error) content.append(refusal(`Could not check your game: ${state.check.error}`));
  if (ok(state.check) && !state.check.ready && state.check.problem) content.append(partial(state.check.problem));
  if (result) {
    content.append(el("div", { class: result.needs_recovery ? "partial" : "said", role: "status" }, el("p", { text: result.message || "Done." })));
    if (result.needs_recovery) content.append(el("a", { href: "recovery.html", class: "button", text: "Troubleshoot this session" }));
  }
  if (ok(state.progress) && state.progress.complete === false && state.check?.ready) {
    content.append(el("div", { class: "partial" }, "Your game is available. ", el("a", { href: "onboarding.html", text: "Finish setting up your preferences" })));
  }
  const home = state.home;
  const pads = ok(state.devices) && Array.isArray(state.devices.pads) ? state.devices.pads : null;
  const mods = ok(state.mods) && Array.isArray(state.mods.activation) ? state.mods.activation : null;
  content.append(el("div", { class: "quick-grid" },
    quickLink("controls.html", "CONTROLLERS", pads ? (pads.length ? `${pads.length} connected` : "No controller") : "Check controllers", "Manage devices"),
    quickLink("mods.html", "MODS", mods ? `${mods.length} active mod${mods.length === 1 ? "" : "s"}` : "Explore your mods", "Manage installed mods"),
    quickLink("#game-options", "GRAPHICS", graphicsLabel(), "Change preset")));
  const details = el("details", { id: "game-options", class: "home-details" }, el("summary", { text: "Game options" }));
  details.open = detailsOpen;
  details.addEventListener("toggle", () => { detailsOpen = details.open; });
  details.append(graphicsBlock(), factsBlock(), el("button", { type: "button", text: "Open Dolphin settings", disabled: busy || externalSession, onclick: openDolphin }));
  content.append(details);
  const hero = heroBlock();
  // The scene is a persistent layer. Recreating it made selection changes flash.
  const art = screen.querySelector?.('.hero-art');
  if (art) hero.querySelector('.hero-art').replaceWith(art);
  screen.replaceChildren(hero, content);
  // Open the fold when arriving from the graphics shortcut.
  if (typeof document.querySelectorAll === "function") for (const link of document.querySelectorAll('a[href="#game-options"]')) {
    link.addEventListener("click", event => {
      event.preventDefault(); details.open = true; detailsOpen = true;
      requestAnimationFrame(() => details.scrollIntoView({block:'start',behavior:matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth'}));
    });
  }
}

async function refresh() {
  try {
    await load();
    if (state.progress?.complete === false && !new URL(location.href).searchParams.has('setup')) { location.replace('onboarding.html'); return; }
  } catch (e) {
    problem = reason(e);
  }
  draw();
}

const pill = document.getElementById("bridge");
if (pill) {
  pill.textContent = bridge ? "Launcher connected" : "Preview";
  pill.className = "pill " + (bridge ? "ok" : "bad");
}
markCurrent();
wireCoreFold();
if (bridge) {
  refresh();
} else {
  problem = "The launcher core is not attached. Open this page inside the launcher window.";
  draw();
}

if (typeof document.addEventListener === "function") {
  document.addEventListener("session-state", e => { state.active = e.detail; externalSession = e.detail; if (!busy) refresh(); });
  document.addEventListener("refresh-devices", () => { if (!busy) refresh(); });
}
