// A focused Play screen. Launch checks remain visible; settings and paths
// live in Game details without moving the artwork or interrupting navigation.

import { bridge, call, el, fact, facts, get, markCurrent, minutes, reason, wireCoreFold } from "./shell.js";

// Opened from a friend's invite link: straight on to Online, which fills in
// the room. Play is the window's first page, so this is where the hand-off
// lives; the code stays put until Online has used it.
async function handOffInvite() {
  try {
    const pending = await call("netplay_pending_join");
    if (!pending || !pending.code) return;
    // Not before setup: Online needs a game to join with. The code stays
    // put, and this runs again once Play comes back after setup.
    const progress = await call("onboarding_progress", { legacyRoot: null });
    if (progress && progress.complete) location.replace("online.html");
  } catch (_) { /* Online is one click away either way */ }
}
if (bridge) {
  handOffInvite();
  // An invite clicked while this launcher was already open: the second
  // launcher hands the code over and brings this window forward.
  window.addEventListener("focus", handOffInvite);
}

// Each read settles on its own: a failed pad scan must not blank the page.
const state = {};
let busy = false;
let playing = false;
let result = null;
let problem = null;
let externalSession = false;
let leagueMatch = false;
// Set when Play was NOT started because player one's pad cannot press Start or
// A. The launcher has always known this before the game opened; it used to
// start the game anyway, and the player found out at a press start screen that
// nothing could get past. Now they are told, and choose.
let controllerSetup = null;
let playAnyway = false;
const signIn = { active: false, starting: false, deviceCode: null, userCode: null, verifyUrl: null, timer: null, message: null, polling: false };

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
    graphicsOptions: call("setup_graphics_options"),
    dolphin: call("dolphin_preferences", { runtime: get("runtime") }),
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
  controllerSetup = null;
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
      // Always sent: false asks first, true is the player's own choice to go on.
      playAnyway,
    });
    if (result && result.started === false) {
      // Not a failure, so never the red "could not start": a setup to offer.
      if (result.controller_setup) controllerSetup = { ...result.controller_setup, message: result.message };
      else problem = result.message || "The game could not start.";
      result = null;
    }
  } catch (e) {
    problem = reason(e);
  }
  playing = false;
  busy = false;
  leagueMatch = false;
  playAnyway = false;
  await load();
  draw();
}

function ok(x) {
  return x && !x.error;
}

async function startSignIn() {
  // Two quick presses must not start two device-code flows: the second
  // would overwrite the first's timer and leave it polling forever.
  if (busy || signIn.active || signIn.starting) return;
  signIn.starting = true;
  signIn.message = null;
  try {
    const s = await call("identity_sign_in_start");
    signIn.active = true; signIn.deviceCode = s.device_code; signIn.userCode = s.user_code;
    // Kept whether or not a browser opened, and drawn by identityCard in both
    // cases. It used to be shown only when opening failed, which meant it was
    // never shown at all: the old opener ran a file manager, and a file manager
    // always starts, so `opened` was true even the time it put Adam's Documents
    // folder on screen instead of the sign-in page.
    signIn.verifyUrl = s.verify_url;
    draw();
    signIn.timer = setInterval(async () => {
      // One poll at a time. The command does not answer until the sign-in is
      // saved, and a poll that is still out is the normal case rather than a
      // rare one. A second poll fired while the first is in flight asks about
      // a device code the server has already consumed, so it gets a refusal,
      // and that refusal used to land on the screen next to the sign-in that
      // was about to succeed.
      if (signIn.polling || !signIn.active) return;
      signIn.polling = true;
      try {
        const r = await call("identity_sign_in_poll", { deviceCode: signIn.deviceCode });
        // stopSignIn with no message clears the message, so an earlier
        // failure cannot sit underneath "Signed in as ...".
        if (r.state === "done") { stopSignIn(); await refresh(); }
        else if (r.state === "expired") { stopSignIn("The sign-in code expired. Try again."); draw(); }
      } catch (e) { stopSignIn(reason(e)); draw(); }
      finally { signIn.polling = false; }
    }, Math.max(2, s.interval || 3) * 1000);
  } catch (e) { signIn.message = reason(e); draw(); }
  finally { signIn.starting = false; }
}

function stopSignIn(message) {
  if (signIn.timer) clearInterval(signIn.timer);
  signIn.timer = null; signIn.active = false; signIn.deviceCode = null; signIn.userCode = null; signIn.verifyUrl = null;
  // Always assigned, never only when there is something to say. Leaving the
  // old text in place is how a successful sign-in ended up displayed under a
  // stale "The sign-in code expired." from a poll that raced it.
  signIn.message = message || null;
}

function identityCard() {
  const id = state.identity;
  const card = el("section", { class: "block play-identity", "aria-label": "Account" });
  if (signIn.active) {
    card.append(el("p", { text: `Waiting for Discord. Your code is ${signIn.userCode}.` }));
    // The address, in every case, not only when the browser did not open. A
    // browser can open on another monitor, behind this window, or in a profile
    // that is not signed into Discord, and the launcher counts all three as
    // success. A player holding a code with nowhere to type it has no way
    // forward, and this is the only thing on the card they can act on alone.
    // One sentence for both cases, because "where to go" is the same sentence
    // whether or not something already went there. The address is not a secret
    // from the person whose code it is.
    if (signIn.verifyUrl) card.append(el("p", { class: "note", text: `Go to ${signIn.verifyUrl} to approve it.` }));
    card.append(el("button", { type: "button", text: "Cancel", onclick: () => { stopSignIn(); draw(); } }));
  } else if (ok(id) && id.signed_in) {
    card.append(el("p", { text: `Signed in as ${id.display_name}` }));
  } else {
    // Never `id.error`. A failed read hands back the command's own name, the
    // credential vault's wording or a path, and none of that is something a
    // player can act on. The one sentence covers every way the read can fail,
    // because from where they are sitting there is no difference between them
    // and nothing different to do about any of them.
    card.append(el("p", { text: ok(id) ? "Sign in with Discord to track your stats from your games." : ACCOUNT_UNREADABLE }));
    card.append(el("button", { type: "button", text: "Sign in with Discord", disabled: busy, onclick: startSignIn }));
  }
  // In every state, not only when signed in. The sentence written for a
  // signed-out player -- their game was recorded and is waiting for them to
  // sign in -- used to be drawn inside the signed-in branch, so the only
  // person it was written for never saw it.
  if (result?.stats_note) card.append(el("p", { class: "note", text: result.stats_note }));
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
  if (message) {
    // The message plus the launch details, for pasting into a forum post or
    // a Discord message, whole, however narrow the window is.
    const copy = el("button", { type: "button", class: "play-copy", text: "Copy error", onclick: async () => {
      const report = [message, factsBlock().textContent.trim()].filter(Boolean).join("\n\n");
      try { await navigator.clipboard.writeText(report); copy.textContent = "Copied"; }
      catch (_) {
        const box = el("textarea", { class: "play-copy-box" }); box.value = report; document.body.append(box); box.select();
        copy.textContent = document.execCommand("copy") ? "Copied" : "Select and copy"; box.remove();
      }
    } });
    dialog.append(el("p", { class: "play-detail-message", text: message }), copy);
  }
  // What the memory cards said around the game (gear rows brought up to
  // date, in-game edits kept, or a card left as it was). Quiet most sessions.
  if (result?.cards_note) dialog.append(el("p", { class: "play-detail-message", text: result.cards_note }));
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
  const history = ok(state.home) && state.home.sessions_played ? `${minutes(state.home.minutes_played)} played` : "Ready to play";
  bar.append(el("div", { class: "play-state" + (error || result?.needs_recovery ? " has-error" : ""), role: error || result?.needs_recovery ? "alert" : "status" },
    el("span", { class: "play-state-title", text: inSession ? "Enjoy the game" : error ? "Your game could not start" : result?.needs_recovery ? "Game closed unexpectedly" : ready ? "Ready to play" : "Setup required" }),
    el("span", { class: "play-history", text: inSession ? "Settings apply to your next session" : error || (result?.needs_recovery ? result.message : null) || history })));
  bar.append(el("button", { type: "button", id: "game-details-open", class: "play-details-button", "aria-haspopup": "dialog", onclick: openDetails },
    el("span", { text: "Game details" }), el("span", { text: "···", "aria-hidden": "true" })));
  controls.append(bar);
  // Tier 2 of the game-image law: a disc this launcher could not verify as
  // a Reborn build, but the player chose to play anyway from Settings. That
  // choice must stay visible and reversible, and the wording here must
  // never suggest a preference the player chose, since it is almost always
  // a fault to repair instead.
  if (controllerSetup) {
    controls.append(el("div", { class: "play-controller-setup", role: "alert" },
      el("strong", { text: "Your controller needs a one-time setup" }),
      el("p", { text: controllerSetup.message }),
      el("div", { class: "play-controller-setup-actions" },
        el("a", { class: "button primary", href: "controls.html", text: "Set up controller" }),
        el("button", { type: "button", id: "play-anyway", disabled: busy, text: "Play anyway", onclick: () => { playAnyway = true; play(); } }))));
  }
  if (state.check?.game_verdict === "unconverted") {
    controls.append(el("div", { class: "play-warning", role: "status" },
      el("span", { text: "Not a Reborn build" }),
      el("a", { class: "play-warning-repair", href: "settings.html#files", text: "Repair" })));
  }
  // The player's own test for a right pack is "can I see stuff, and what
  // version is it". This is that answer, read straight off the folder the
  // launch will actually use, not asked about separately.
  if (state.check?.pack) {
    const pack = state.check.pack;
    if (pack.reachable) {
      controls.append(el("p", { class: "note", text: pack.version ? `Texture pack ${pack.version}` : "Texture pack loaded" }));
    } else {
      controls.append(el("div", { class: "play-warning", role: "status" },
        el("span", { text: "Texture pack not loading" })));
    }
  }
  controls.append(identityCard());
  const pads = ok(state.devices) && Array.isArray(state.devices.pads) ? state.devices.pads : null;
  const overview = el("nav", { class: "play-overview", "aria-label": "Game preferences" });
  for (const [href, title, value] of [
    ["controls.html", "Controllers", pads ? pads.length ? `${pads.length} connected` : "Connect a controller" : "Manage devices"],
    ["settings.html#graphics", "Graphics", graphicsLabel()],
  ]) overview.append(el("a", { href }, el("span", { text: title }), el("strong", { text: value }), el("span", { class: "play-overview-arrow", text: "\u203a", "aria-hidden": "true" })));
  const dolphin = ok(state.dolphin) ? state.dolphin : null;
  const fullscreen = String(dolphin?.values?.fullscreen ?? "True").toLowerCase() === "true";
  const displayLocked = busy || inSession || !dolphin || dolphin.editable === false;
  const display = el("input", {type:"checkbox", id:"play-fullscreen", checked:fullscreen, disabled:displayLocked});
  display.addEventListener("change", async () => {
    display.disabled = true;
    try {
      await call("dolphin_preference_set", {runtime:get("runtime"), key:"fullscreen", value:String(display.checked)});
      dolphin.values.fullscreen = String(display.checked);
    } catch (e) { display.checked = fullscreen; problem = reason(e); draw(); }
    finally { display.disabled = displayLocked; }
  });
  overview.append(el("label", {class:"play-fullscreen", for:"play-fullscreen"}, display, el("span", {text:"Fullscreen"})));
  const dialog = detailsDialog();
  screen.replaceChildren(scene, controls, overview, dialog);
  if (wasOpen) { dialog.showModal(); if (filesOpen) dialog.querySelector("details").open = true; }
  else if (focusedId) { const target = document.getElementById(focusedId); if (target && !target.disabled) target.focus({preventScroll:true}); }
}

async function refresh() {
  try {
    // Setup first, before anything else can fail or take its time: an
    // unfinished first run belongs in the wizard, whatever else this page
    // cannot load. The check after load() stays as the fallback.
    if (bridge && !/(^|[?&])setup(=|&|$)/.test(location.search || "")) {
      const progress = await call("onboarding_progress", { legacyRoot: null }).catch(() => null);
      if (progress?.complete === false) { location.replace('onboarding.html'); return; }
    }
    // Setup held the window at one size; the launcher proper is resizable.
    if (bridge) call("setup_window_lock", { locked: false }).catch(() => {});
    await load();
    if (state.progress?.complete === false && !/(^|[?&])setup(=|&|$)/.test(location.search || "")) { location.replace('onboarding.html'); return; }
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
  document.addEventListener("session-state", e => {
    if (typeof e.detail !== "boolean") return;
    const changed = externalSession !== e.detail;
    state.active = externalSession = e.detail;
    if (changed && !busy) refresh();
  });
  document.addEventListener("refresh-devices", () => { if (!busy) refresh(); });
}
