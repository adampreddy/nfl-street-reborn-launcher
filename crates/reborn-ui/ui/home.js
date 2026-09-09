// A focused Play screen. Launch checks remain visible; settings and paths
// live in Game details without moving the artwork or interrupting navigation.

import { bridge, call, el, fact, facts, get, markCurrent, minutes, reason, wireCoreFold } from "./shell.js";

// Each read settles on its own: a failed pad scan must not blank the page.
const state = {};
let busy = false;
let playing = false;
let result = null;
let problem = null;
let externalSession = false;
let leagueMatch = false;
const signIn = { active: false, deviceCode: null, userCode: null, timer: null, message: null };

// Shown when `identity_status` fails, whatever the reason. The same sentence
// the core uses for the same situation, so a player never sees two wordings
// for one state.
const ACCOUNT_UNREADABLE = "Whether you are signed in could not be read just now. Try again in a moment.";

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
    identity: call("identity_status"),
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
      leagueMatch,
    });
    if (result && result.started === false) { problem = result.message || "The game could not start."; result = null; }
  } catch (e) {
    problem = reason(e);
  }
  playing = false;
  busy = false;
  leagueMatch = false;
  await load();
  draw();
}

function ok(x) {
  return x && !x.error;
}

async function startSignIn() {
  if (busy || signIn.active) return;
  signIn.message = null;
  try {
    const s = await call("identity_sign_in_start");
    signIn.active = true; signIn.deviceCode = s.device_code; signIn.userCode = s.user_code;
    if (!s.opened) signIn.message = `Open ${s.verify_url} in a browser.`;
    draw();
    signIn.timer = setInterval(async () => {
      try {
        const r = await call("identity_sign_in_poll", { deviceCode: signIn.deviceCode });
        if (r.state === "done") { stopSignIn(); await refresh(); }
        else if (r.state === "expired") { stopSignIn("The sign-in code expired. Try again."); draw(); }
      } catch (e) { stopSignIn(reason(e)); draw(); }
    }, Math.max(2, s.interval || 3) * 1000);
  } catch (e) { signIn.message = reason(e); draw(); }
}

function stopSignIn(message) {
  if (signIn.timer) clearInterval(signIn.timer);
  signIn.timer = null; signIn.active = false; signIn.deviceCode = null; signIn.userCode = null;
  if (message) signIn.message = message;
}

function identityCard() {
  const id = state.identity;
  const card = el("section", { class: "block play-identity", "aria-label": "Account" });
  if (signIn.active) {
    card.append(el("p", { text: `Waiting for Discord. Your code is ${signIn.userCode}.` }));
    card.append(el("button", { type: "button", text: "Cancel", onclick: () => { stopSignIn(); draw(); } }));
  } else if (ok(id) && id.signed_in) {
    card.append(el("p", { text: `Signed in as ${id.display_name}` }));
    if (result?.stats_note) card.append(el("p", { class: "note", text: result.stats_note }));
  } else {
    // Never `id.error`. A failed read hands back the command's own name, the
    // credential vault's wording or a path, and none of that is something a
    // player can act on. The one sentence covers every way the read can fail,
    // because from where they are sitting there is no difference between them
    // and nothing different to do about any of them.
    card.append(el("p", { text: ok(id) ? "Sign in with Discord to record your games." : ACCOUNT_UNREADABLE }));
    card.append(el("button", { type: "button", text: "Sign in with Discord", disabled: busy, onclick: startSignIn }));
  }
  if (signIn.message) card.append(el("p", { class: "note", text: signIn.message }));
  return card;
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
  if (on.length === 0) return { text: "none selected", note: null };
  const named = on.map((a) => {
    const p = (m.packages || []).find((x) => x.id === a.id);
    return p ? p.name : `${a.id} (not installed)`;
  });
  return { text: `${on.length} selected: ${named.join(", ")}`, note: null };
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

// The artwork is independent of the action bar and all transient messages.
function openDetails() {
  document.getElementById("game-details").showModal();
}

function detailsDialog() {
  const dialog = el("dialog", { id: "game-details", class: "play-dialog", "aria-labelledby": "game-details-title" });
  dialog.append(el("div", { class: "play-dialog-heading" },
    el("h2", { id: "game-details-title", text: "Game details" }),
    el("button", { type: "button", text: "Close", onclick: () => dialog.close() })));
  const pads = ok(state.devices) && Array.isArray(state.devices.pads) ? state.devices.pads : null;
  const mods = ok(state.mods) && Array.isArray(state.mods.activation) ? state.mods.activation : null;
  const links = el("div", { class: "play-detail-links" });
  for (const [href, title, value] of [
    ["settings.html#graphics", "Graphics", graphicsLabel()],
    ["controls.html", "Controllers", pads ? `${pads.length} connected` : "Check devices"],
    ["mods.html", "Mods", mods ? `${mods.length} selected` : "Manage mods"],
  ]) links.append(el("a", { href }, el("span", { text: title }), el("span", { class: "play-detail-value", text: value }), el("span", { text: "›", "aria-hidden": "true" })));
  dialog.append(links);
  const message = problem || state.check?.error || state.check?.problem || result?.message;
  if (message) dialog.append(el("p", { class: "play-detail-message", text: message }));
  if (problem || state.check?.error || state.check?.problem || result?.needs_recovery) dialog.append(el("a", { class: "play-help", href: "recovery.html", text: "Troubleshoot this session" }));
  const files = el("details", { class: "play-files" }, el("summary", { text: "Files & launch details" }), factsBlock());
  dialog.append(files, el("div", { class: "play-dialog-actions" },
    el("a", { href: "settings.html#files", text: "Manage files" }),
    el("button", { type: "button", text: "Open Dolphin settings", disabled: busy || externalSession, onclick: () => { dialog.close(); openDolphin(); } })));
  dialog.addEventListener("close", () => document.getElementById("game-details-open")?.focus());
  dialog.addEventListener("click", event => { if (event.target === dialog) { const r = dialog.getBoundingClientRect(); if (event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom) dialog.close(); } });
  return dialog;
}

function draw() {
  if (typeof state.active === "boolean") externalSession = state.active;
  const screen = document.getElementById("screen");
  // Background refreshes must not close an open dialog or discard its focus.
  const oldDialog = document.getElementById("game-details");
  const wasOpen = !!oldDialog?.open;
  const filesOpen = oldDialog?.querySelector?.("details")?.open;
  const focusedId = document.activeElement?.id;
  const ready = ok(state.check) && state.check.ready;
  const inSession = playing || externalSession;
  const scene = el("section", { class: "play-scene", "aria-label": "NFL Street Reborn" },
    el("div", { class: "hero-art", "aria-hidden": "true" }, el("img", { src: "art/play-background.png", alt: "" })),
    el("h1", { class: "play-wordmark" }, el("img", { src: "art/play-logo.png", alt: "NFL Street Reborn" })));
  const art = screen.querySelector?.(".hero-art");
  if (art) scene.querySelector(".hero-art").replaceWith(art);
  const controls = el("div", { class: "play-controls" });
  const error = problem || state.check?.error || (!ready && state.check?.problem);
  const bar = el("div", { class: "play-bar" });
  if (ready || inSession) bar.append(el("button", { type: "button", id: "play-now", "data-primary": "true", class: "play", disabled: busy || inSession, onclick: play },
    el("span", { class: "play-glyph", text: inSession ? "" : "▶", "aria-hidden": "true" }),
    el("span", { text: inSession ? "Session active" : busy ? "Opening…" : "Play now" })));
  else bar.append(el("a", { href: "onboarding.html", class: "button primary play", "data-primary": "true", text: "Set up your game" }));
  const history = ok(state.home) && state.home.sessions_played ? `${minutes(state.home.minutes_played)} played` : "NFL Street 2 · GameCube";
  bar.append(el("div", { class: "play-state" + (error || result?.needs_recovery ? " has-error" : ""), role: error || result?.needs_recovery ? "alert" : "status" },
    el("span", { class: "play-state-title", text: inSession ? "Enjoy the game" : error ? "Your game could not start" : result?.needs_recovery ? "Game closed unexpectedly" : ready ? "Ready to play" : "Setup required" }),
    el("span", { class: "play-history", text: inSession ? "Settings apply to your next session" : error || (result?.needs_recovery ? result.message : null) || history })));
  bar.append(el("button", { type: "button", id: "game-details-open", class: "play-details-button", "aria-haspopup": "dialog", onclick: openDetails },
    el("span", { text: "Game details" }), el("span", { text: "···", "aria-hidden": "true" })));
  controls.append(bar);
  controls.append(identityCard());
  const pads = ok(state.devices) && Array.isArray(state.devices.pads) ? state.devices.pads : null;
  const overview = el("nav", { class: "play-overview", "aria-label": "Game preferences" });
  for (const [href, title, value] of [
    ["controls.html", "Controllers", pads ? pads.length ? `${pads.length} connected` : "Connect a controller" : "Manage devices"],
    ["settings.html#graphics", "Graphics", graphicsLabel()],
  ]) overview.append(el("a", { href }, el("span", { text: title }), el("strong", { text: value }), el("span", { class: "play-overview-arrow", text: "\u203a", "aria-hidden": "true" })));
  const dialog = detailsDialog();
  screen.replaceChildren(scene, controls, overview, dialog);
  if (wasOpen) { dialog.showModal(); if (filesOpen) dialog.querySelector("details").open = true; }
  else if (focusedId) { const target = document.getElementById(focusedId); if (target && !target.disabled) target.focus({preventScroll:true}); }
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
