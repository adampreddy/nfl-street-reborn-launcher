// Settings, as a page: where things live, and the few paths a person can set.
//
// Every setting waits in one list until Save changes (spec §7, §8.2): Dolphin's options, the graphics
// preset, the update preference, the locations, and the launcher window's look. Actions (Open Dolphin,
// check for updates, repair, uninstall) still happen when pressed; they are not settings. The one
// exception is live audio: while a game is running, volume and mute reach it at once, which is what
// they have always done.

import { bridge, call, cleanPath, el, get, icon, markCurrent, partial, reason, refusal, set, versionLine, wireCoreFold } from "./shell.js";
import { applyLook, refreshLookResult } from "./look.js";
import { mountSaveBar } from "./savebar.js";
import { tabs } from "./tabs.js";
import { createDraft, HELD } from "./settings-draft.js";

const state = {};
// What changed in each release, on the site. Opened in the browser.
const PATCH_NOTES_URL = "https://nflstreetreborn.com/blog";
const TABS = ["General", "Graphics", "Display", "Audio", "Saves", "Updates", "Files & storage"];
const tabForHash = () => ({ general: "General", graphics: "Graphics", display: "Display", audio: "Audio", saves: "Saves", updates: "Updates", files: "Files & storage" })[location.hash?.slice(1).toLowerCase()] || "General";
let settingsTab = tabForHash();
window.addEventListener?.("hashchange", () => { if (!busy) { settingsTab = tabForHash(); draw(); } });
let problem = null;
let busy = false;
// The update flow's last answers, shown in place.
let update = null; // update_check view
let downloaded = null; // update_download view
// What Open Dolphin last said, in the Dolphin card.
let dolphinSaid = null;
// The Tier 2 choice, while it is open. Set by a save of the game path that stored nothing because the
// chosen image is NFL Street 2 but not a Reborn build. The words come from the backend so the launch
// refusal, the Play badge and this card all describe one disc. Never "custom": this is a fault to
// repair, not a preference anybody expressed.
let unverified = null;
// Every change not saved yet (spec §7), and the bar that shows them.
const draft = createDraft();
let saveBar = null, lastChangedId = "";
// Launcher window (spec §4.3, §5.3): previews live, saved through the same bar.
let look = null, lookError = null, lookSaved = null, lookDraft = null, lookBusy = false, lookMsg = "";

function ok(x) {
  return x && !x.error;
}

async function load() {
  const reads = {
    dolphin: call("dolphin_preferences", { runtime: get("runtime") }),
    graphics: call("setup_graphics_options"),
    hardware: call("setup_hardware_info"),
    progress: call("onboarding_progress"),
    preferences: call("settings_paths"),
    installation: call("setup_install_info"),
    paths: call("paths"),
    check: call("launch_check", { runtime: get("runtime"), game: get("game"), userDir: get("userDir") }),
    identity: call("identity_status"),
    policy: call("update_policy"),
    queue: call("stats_queue"),
    content: call("content_status"),
  };
  const keys = Object.keys(reads);
  const settled = await Promise.allSettled(Object.values(reads));
  keys.forEach((k, i) => {
    state[k] = settled[i].status === "fulfilled" ? settled[i].value : { error: reason(settled[i].reason) };
  });
}

// ---- the save bar -------------------------------------------------------------------------------

// Busy for the page: actions refuse to start, and #screen says aria-busy (which update-notice.js also
// reads, so nothing installs mid-save). The shell's setLauncherBusy also disables the page's controls.
function holdPage(on) {
  busy = on;
  const screen = document.getElementById("screen");
  if (on) screen?.setAttribute("aria-busy", "true"); else screen?.removeAttribute("aria-busy");
  window.setLauncherBusy?.(on);
}

function refreshBar() {
  const changes = draft.changes();
  if (!saveBar && !changes.length) return;
  if (!saveBar) {
    const lane = document.getElementById("save-lane");
    if (!lane) return;
    saveBar = mountSaveBar(lane, { onSave: saveAll, onDiscard: discardAll, returnFocus: () => document.getElementById(lastChangedId) });
  }
  saveBar.update(changes);
}

// Record one change; `focusId` is the control that made it, so focus goes back to it after the redraw.
function stage(key, focusId, entry) {
  draft.put(key, entry);
  if (focusId) lastChangedId = focusId;
  draw();
  refreshBar();
  if (focusId) document.getElementById(focusId)?.focus({ preventScroll: true });
}

// Save everything, in the order it changed. A failure is thrown so the bar says which and why; a
// change held for the Tier 2 choice is not a failure (the dialog asks), and it stays pending.
// The page is busy for the whole save, so no action (Open Dolphin, Check for updates, Repair…) runs in the
// middle of it; busy is cleared in finally, so a failed save leaves the page usable.
async function saveAll() {
  holdPage(true);
  let result;
  try { result = await draft.saveAll(); }
  finally {
    try { await load(); }
    finally {
      holdPage(false);
      draw();
      refreshBar();
    }
  }
  if (result.stopped && result.stopped.reason !== HELD) throw new Error(`${result.stopped.label}: ${result.stopped.reason}`);
}

// Put everything back. A revert that fails throws its reason, which the bar shows; that change and the
// older ones stay pending and the bar stays open, so nothing still previewed is shown as discarded.
async function discardAll() {
  holdPage(true);
  try { await draft.discardAll(); }
  finally {
    holdPage(false);
    unverified = null;
    // The look switches show what is still pending, not what was hoped for.
    if (lookSaved && lookDraft) {
      lookDraft.windowGlass = draft.value("look.glass", lookSaved.windowGlass);
      lookDraft.slowAnimations = draft.value("look.motion", lookSaved.slowAnimations);
    }
    draw();
    refreshBar();
  }
}

// ---- building blocks ----------------------------------------------------------------------------

// One glass section: a title, an optional lead line, then rows (mockup .section).
function section(title, lead, ...children) {
  return el("section", { class: "glass pref-section" }, el("h2", { text: title }), lead ? el("p", { class: "pref-lead", text: lead }) : null, ...children);
}

// One row: label | value | control, with the help line under the value (mockup .row).
function row(label, { id = null, cls = "", value = null, control = null, help = null } = {}) {
  return el("div", { class: "pref-row" + (cls ? " " + cls : "") },
    id ? el("label", { class: "pref-label", for: id, text: label }) : el("span", { class: "pref-label", text: label }),
    el("div", { class: "pref-value" }, value),
    el("div", { class: "pref-control" }, ...[].concat(control)),
    help ? el("p", { class: "pref-help", text: help }) : null);
}

function toggle(id, label, on, disabled, onFlip) {
  return el("button", { type: "button", id, class: "look-switch", role: "switch", "aria-label": label, "aria-checked": String(on), disabled, onclick: onFlip });
}
const onOff = (on) => el("span", { class: "pref-state", text: on ? "On" : "Off" });
const mb = (b) => Math.max(1, Math.round((b || 0) / 1048576));

async function act(label, cmd, args, after) {
  if (busy) return;
  const focusedId = document.activeElement?.id;
  busy = true;
  problem = null;
  if (window.setLauncherBusy) window.setLauncherBusy(true); else draw();
  const messages = { update_check: "Checking for updates…", update_download: "Downloading and verifying…", update_apply: "Starting installer…", content_check: "Checking game files…", content_fetch: "Downloading game content…", content_promote: "Installing game content…", content_repair_image: "Repairing game files…" };
  if (messages[cmd]) document.getElementById("screen").append(el("p", { class: "settings-status", role: "status", text: messages[cmd] }));
  try {
    const r = await call(cmd, args);
    after(r);
  } catch (e) {
    problem = `${label}: ${reason(e)}`;
  }
  await load();
  busy = false;
  window.setLauncherBusy?.(false);
  draw();
  if (focusedId) document.getElementById(focusedId)?.focus({ preventScroll: true });
}

// ---- the Dolphin card ---------------------------------------------------------------------------

function dolphinCard() {
  return el("section", { class: "glass dolphin-card", "aria-label": "NFL Street Reborn Dolphin" },
    el("span", { class: "dolphin-icon", "aria-hidden": "true" }, icon(["M5 4h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z", "M8 21h8M12 17v4"])),
    el("div", { class: "dolphin-text" },
      el("h2", { text: "NFL Street Reborn Dolphin" }),
      el("p", { text: "Everyday settings below. Open Dolphin for its complete set of tools. Reborn's textures apply to any NFL Street 2 disc you open here, so another copy will show Reborn's players." }),
      el("p", { class: "dolphin-said", role: "status", text: dolphinSaid || "" })),
    el("button", { type: "button", class: "btn", text: "Open Dolphin", disabled: busy, onclick: openDolphin }));
}

async function openDolphin() {
  if (busy) return;
  try {
    await call("open_runtime", { runtime: get("runtime"), userDir: get("userDir") });
    dolphinSaid = "Dolphin opened.";
    await refresh();
  } catch (e) { dolphinSaid = reason(e); draw(); }
}

// ---- Dolphin's options (General, Graphics, Display, Audio, Saves) --------------------------------

const preferenceGroups = {
  General: [["confirm_stop", "Confirm before stopping", "Ask before closing a game.", "bool"], ["pause_unfocused", "Pause when unfocused", "Pause local play when you switch to another app.", "bool"], ["osd_messages", "Game notifications", "Show Dolphin messages over the game.", "bool"]],
  Graphics: [["internal_resolution", "Internal resolution", "How sharp the game renders. 3× is the Reborn default; lower it if the game runs slowly.", "resolution"], ["msaa", "Anti-aliasing", "Smooths edges. Off is the Reborn default and is what the game has always run at; raising it costs frame rate.", "msaa"], ["video_backend", "Graphics backend", "Direct3D 11 is the Reborn default. Vulkan or OpenGL can run better on some drivers.", "video_backend"], ["sync_gpu", "Synchronize GPU thread", "Off is faster. Turn it on only if you see graphical glitches.", "bool"], ["vsync", "V-Sync", "Reduce screen tearing. May add a small amount of input delay.", "bool"], ["show_fps", "Frame rate", "Show a small frames-per-second counter.", "bool"]],
  Display: [["fullscreen", "Start in fullscreen", "Open the game in fullscreen mode.", "bool"], ["show_ping", "Netplay ping", "Show connection latency during online play.", "bool"], ["always_on_top", "Keep game on top", "Keep the game window above other windows.", "bool"]],
  Audio: [["audio_backend", "Audio output", "Default shares audio with other apps. Windows exclusive gives Dolphin sole use of the output device.", "backend"], ["volume", "Game volume", "Adjust Dolphin audio output.", "range", 0, 100], ["muted", "Mute audio", "Silence game audio without changing the volume.", "bool"], ["audio_stretch", "Audio stretching", "Smooth out audio during brief slowdowns.", "bool"], ["audio_latency", "Audio buffer", "Audio buffering in milliseconds. Raise it if sound crackles.", "range", 0, 200]],
  Saves: [["slot_a", "Memory Slot A", "Choose where Player 1 saves game progress.", "slot"], ["card_a", "Slot A memory card", "Used when Slot A is set to Memory card. Leave blank for Dolphin’s default.", "card"], ["gci_a", "Slot A save folder", "Used when Slot A is set to GCI folder. Leave blank for the default folder.", "folder"], ["slot_b", "Memory Slot B", "An optional second card. Existing saves are kept when you change the type.", "slot"], ["card_b", "Slot B memory card", "Used when Slot B is set to Memory card.", "card"], ["gci_b", "Slot B save folder", "Used when Slot B is set to GCI folder.", "folder"]],
};

// The value a control shows: the pending one if there is one, else what Dolphin has.
function prefValue(key) { return String(draft.value("pref." + key, state.dolphin?.values?.[key] ?? "")); }

function prefEntry(key, label, type, value) {
  const saved = String(state.dolphin.values[key] ?? "");
  return { label, value,
    same: type === "bool" ? value.toLowerCase() === saved.toLowerCase() : value === saved,
    save: async (v) => { await call("dolphin_preference_set", { runtime: get("runtime"), key, value: v }); state.dolphin.values[key] = v; } };
}
function stagePref(key, label, type, value, focusId) { stage("pref." + key, focusId, prefEntry(key, label, type, value)); }
// Leaving a typed field: record the change without redrawing, so the click that moved focus still lands.
function quietPref(key, label, type, value, focusId) { draft.put("pref." + key, prefEntry(key, label, type, value)); lastChangedId = focusId; refreshBar(); }

// A path box a person can type or paste into (spec §10). Enter stages it and redraws; leaving the box
// stages it quietly. Quotes around a pasted path are dropped (cleanPath). Nothing typed, nothing staged.
function typedPath(input, shown, onValue) {
  let last = shown;
  const take = (loud) => { const v = cleanPath(input.value); if (v !== last) { last = v; onValue(v, loud); } };
  input.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault?.(); take(true); } });
  input.addEventListener("blur", () => take(false));
}

// Volume and mute reach a running game at once (spec §10: what they have always done), so while a game
// is open they are not held for the save bar.
async function saveLive(key, label, value, focusId) {
  let said;
  try {
    await call("dolphin_preference_set", { runtime: get("runtime"), key, value });
    state.dolphin.values[key] = value; said = `${label} saved.`;
    // What the game has now is newer than anything staged before it went live: a later Save must not
    // write the old value over it.
    draft.drop("pref." + key);
  }
  catch (e) { said = reason(e); }
  draw();
  refreshBar();
  const status = document.getElementById("dolphin-status");
  if (status) status.textContent = said;
  document.getElementById(focusId)?.focus({ preventScroll: true });
}

function selectFor(type, id, current, disabled) {
  const choices = {
    backend: [["Cubeb", "Default (shared audio)"], ["WASAPI (Exclusive Mode)", "Windows exclusive"]],
    slot: [["8", "GCI folder"], ["1", "Memory card"], ["255", "Nothing"]],
    resolution: [["1", "1× native (640×528)"], ["2", "2×"], ["3", "3× (Reborn default)"], ["4", "4×"], ["5", "5×"], ["6", "6×"]],
    video_backend: [["D3D", "Direct3D 11 (Reborn default)"], ["D3D12", "Direct3D 12"], ["Vulkan", "Vulkan"], ["OGL", "OpenGL"]],
    msaa: [["1", "Off (Reborn default)"], ["2", "2×"], ["4", "4×"], ["8", "8×"]],
  }[type] || [];
  const v = type === "resolution" ? String(parseInt(current) || 3) : type === "msaa" ? String(parseInt(current) || 1) : current;
  const select = el("select", { id, disabled }, ...choices.map(([value, text]) => el("option", { value, text })));
  if (!choices.some(([value]) => value === v)) select.append(el("option", { value: v, text: type === "slot" ? `Current device (${v})` : type === "resolution" || type === "msaa" ? `${v}×` : v }));
  select.value = v;
  return select;
}

function prefRow(key, label, description, type, min, max, disabled, isLive) {
  const id = "preference-" + key, current = prefValue(key);
  const commit = (value, focusId = id) => isLive ? saveLive(key, label, value, focusId) : stagePref(key, label, type, value, focusId);
  if (type === "bool") {
    const on = current.toLowerCase() === "true";
    return row(label, { id, value: onOff(on), control: toggle(id, label, on, disabled, () => commit(String(!on))), help: description });
  }
  if (type === "range") {
    const unit = key === "volume" ? "%" : " ms";
    const slider = el("input", { id, type: "range", min, max, step: 1, value: current, disabled });
    const out = el("output", { for: id, class: "pref-state", text: current + unit });
    slider.addEventListener("input", () => { out.textContent = slider.value + unit; });
    slider.addEventListener("change", () => commit(slider.value));
    return row(label, { id, value: slider, control: out, help: description });
  }
  if (type === "card" || type === "folder") {
    const pick = async () => {
      try { const p = await call("pick_path", { kind: type === "card" ? "memory_card" : "folder" }); if (p) commit(p, `change-${key}`); }
      catch (e) { problem = reason(e); draw(); }
    };
    // Editable, so a path can be typed or pasted (spec §10); Enter or leaving the field stages it.
    const input = el("input", { id, type: "text", value: current, placeholder: "Use default", spellcheck: "false", disabled });
    typedPath(input, current, (v, loud) => loud ? commit(v) : quietPref(key, label, type, v, id));
    return row(label, { id, cls: "path-entry",
      value: el("div", { class: "field" }, input),
      control: [
        el("button", { type: "button", id: `change-${key}`, class: "btn", text: "Change…", "aria-label": `Change ${label}`, disabled, onclick: pick }),
        current ? el("button", { type: "button", class: "btn quiet sm", text: "Reset", "aria-label": `Reset ${label} to the default`, disabled, onclick: () => commit("", `change-${key}`) }) : null,
      ], help: description });
  }
  const select = selectFor(type, id, current, disabled);
  select.addEventListener("change", () => commit(select.value));
  return row(label, { id, control: select, help: description });
}

async function openSaves() {
  try { await call("dolphin_saves_folder", { runtime: get("runtime") }); }
  catch (e) { const s = document.getElementById("dolphin-status"); if (s) s.textContent = reason(e); }
}

function dolphinBlock() {
  const title = settingsTab === "Saves" ? "Memory cards & save data" : settingsTab === "Graphics" ? "Rendering" : settingsTab;
  if (!ok(state.dolphin)) return section(title, null, el("p", { class: "partial", text: state.dolphin?.error || "Dolphin settings could not be read." }));
  const d = state.dolphin, values = d.values || {}, live = d.live_editable || [];
  const canEdit = (key) => d.editable !== false || live.includes(key);
  const lead = el("p", { class: "pref-lead", id: "dolphin-status", role: "status", text: settingsTab === "Audio" && live.length
    ? "Volume and mute change as you set them, even during NetPlay. Everything else waits for Save changes."
    : "Changes wait for Save changes and apply the next time you open Dolphin." });
  const rows = [];
  if (d.editable === false) rows.push(el("p", { class: "partial", text: d.readonly_reason || "Close Dolphin to edit these settings, then refresh." }));
  for (const [key, label, description, type, min, max] of preferenceGroups[settingsTab] || []) {
    if (key === "audio_latency" && !["WASAPI (Exclusive Mode)", "OpenAL"].includes(prefValue("audio_backend"))) continue;
    if (values[key] === undefined) { rows.push(el("p", { class: "partial", text: label + " is unavailable. Refresh settings to try again." })); continue; }
    rows.push(prefRow(key, label, description, type, min, max, busy || !canEdit(key), live.includes(key)));
  }
  if (settingsTab === "Saves") rows.push(row("Save folder", { control: el("button", { type: "button", class: "btn", text: "Open save folder", onclick: openSaves }) }));
  return el("section", { class: "glass pref-section" }, el("h2", { text: title }), lead, ...rows);
}

// ---- Graphics ------------------------------------------------------------------------------------

const GRAPHICS_PRESETS = [["performance", "Performance"], ["balanced", "Balanced"], ["quality", "All-Out"]];
function graphicsBlock() {
  const parts = [];
  if (!ok(state.graphics)) parts.push(section("Visual effects", null, el("p", { class: "partial", text: state.graphics?.error || "Graphics settings are unavailable. Refresh settings to try again." })));
  else {
    const saved = get("preset") || state.progress?.summary?.graphics || "balanced";
    const current = state.graphics.current;
    const selected = current ? (current.label === "Previous settings" ? "previous" : "custom") : (saved === "ultra" ? "quality" : GRAPHICS_PRESETS.some((x) => x[0] === saved) ? saved : "balanced");
    const chosen = draft.value("graphics", selected);
    const select = el("select", { id: "graphics-preset", disabled: busy || state.dolphin?.editable === false }, ...GRAPHICS_PRESETS.map(([value, text]) => el("option", { value, text })));
    if (state.graphics.previous || selected === "previous") select.append(el("option", { value: "previous", text: "Previous settings" }));
    if (selected === "custom") select.append(el("option", { value: "custom", text: "Custom", disabled: true }));
    select.value = chosen;
    select.addEventListener("change", () => stage("graphics", "graphics-preset", { label: "Graphics preset", value: select.value, same: select.value === selected,
      save: async (v) => {
        await call("setup_save_graphics_options", { mode: v === "previous" ? "previous" : "preset", resolution: 3, msaa: 1, ssaa: false });
        if (v !== "previous") set("preset", v);
      } }));
    // The ReShade descriptions were stale (off since 3.0.20); the row says what the choice does now.
    const help = chosen === "previous" ? "Use the graphics settings imported from your previous installation." : chosen === "custom" ? "Your saved custom settings are selected." : "Applies the next time you play.";
    const rows = [row("Graphics preset", { id: "graphics-preset", control: select, help })];
    if (ok(state.hardware) && state.hardware.description) {
      const recommended = GRAPHICS_PRESETS.find((x) => x[0] === state.hardware.recommended)?.[1];
      rows.push(row("Your PC", { value: state.hardware.description, control: recommended ? el("span", { class: "pref-state", text: `${recommended} recommended` }) : null }));
    }
    parts.push(section("Visual effects", null, ...rows));
  }
  parts.push(dolphinBlock());
  return parts;
}

// ---- Display: the launcher window ------------------------------------------------------------------

// One guarded step: ignored while another is in flight; a failure is shown, never thrown.
async function lookStep(id, fn) {
  if (lookBusy) return;
  lookBusy = true; lookMsg = "";
  try { await fn(); } catch (e) { lookMsg = reason(e); }
  finally {
    lookBusy = false;
    if (id) lastChangedId = id;
    draw();
    refreshBar();
    if (id) document.getElementById(id)?.focus({ preventScroll: true });
  }
}

const glassOnNow = () => document.documentElement.dataset.glass === "on";

const flipGlass = () => lookStep("look-glass", async () => {
  const next = !lookDraft.windowGlass;
  const v = await call("window_glass_apply", { enabled: next });
  look = v; lookDraft.windowGlass = next;
  applyLook({ ...v, slowAnimations: lookDraft.slowAnimations });
  draft.put("look.glass", { label: "Window glass", value: next, same: next === lookSaved.windowGlass,
    save: async (value) => {
      // Both fields, as always; the other one is sent as it is saved, so this entry changes only its own.
      const saved = await call("launcher_look_set", { windowGlass: value, slowAnimations: lookSaved.slowAnimations });
      look = saved; lookSaved.windowGlass = saved.windowGlass;
      applyLook({ ...saved, slowAnimations: lookDraft.slowAnimations });
    },
    revert: async () => {
      const back = await call("window_glass_apply", { enabled: lookSaved.windowGlass });
      look = back; lookDraft.windowGlass = lookSaved.windowGlass;
      applyLook({ ...back, slowAnimations: lookDraft.slowAnimations });
    } });
});

const flipMotion = () => lookStep("look-motion", async () => {
  lookDraft.slowAnimations = !lookDraft.slowAnimations;
  applyLook({ glassApplied: glassOnNow(), slowAnimations: lookDraft.slowAnimations });
  draft.put("look.motion", { label: "Slow animations", value: lookDraft.slowAnimations, same: lookDraft.slowAnimations === lookSaved.slowAnimations,
    save: async (value) => {
      const saved = await call("launcher_look_set", { windowGlass: lookSaved.windowGlass, slowAnimations: value });
      look = saved; lookSaved.slowAnimations = saved.slowAnimations;
      applyLook({ glassApplied: glassOnNow(), slowAnimations: lookDraft.slowAnimations });
    },
    revert: async () => {
      lookDraft.slowAnimations = lookSaved.slowAnimations;
      applyLook({ glassApplied: glassOnNow(), slowAnimations: lookSaved.slowAnimations });
    } });
});

function lookBlock() {
  if (!look && lookError) return el("p", { class: "partial", role: "alert", text: "Couldn’t read the launcher window settings: " + reason(lookError) });
  if (!look) return el("p", { class: "partial", text: "Reading the launcher window settings…" });
  const glassOn = lookDraft.windowGlass && look.glassSupported;
  const failed = look.glassSupported && lookDraft.windowGlass && !look.glassApplied;
  return el("section", { class: "glass pref-section look-section", "aria-label": "Launcher window" },
    el("h2", { text: "Launcher window" }),
    el("p", { class: "pref-lead", text: "How the launcher itself looks. The game is not affected." }),
    row("Window glass", { value: onOff(glassOn), control: toggle("look-glass", "Window glass", glassOn, !look.glassSupported, flipGlass),
      help: failed ? "The window glass could not be turned on." : look.glassNote || "Frosted glass that picks up your desktop colours. Turn it off for a solid dark background." }),
    row("Slow animations", { value: onOff(lookDraft.slowAnimations), control: toggle("look-motion", "Slow animations", lookDraft.slowAnimations, false, flipMotion),
      help: "Slower, smoother movement when you change pages, open menus and expand matches. Turn it off for quick animations. Windows’ reduce-motion setting always turns movement off." }),
    lookMsg ? el("p", { class: "look-error", role: "status", text: lookMsg }) : null);
}

async function loadLook() {
  const r = await refreshLookResult();
  look = r.view; lookError = r.error;
  if (look) { lookSaved = { windowGlass: look.windowGlass, slowAnimations: look.slowAnimations }; lookDraft = { ...lookSaved }; }
  draw();
}

// ---- Updates ---------------------------------------------------------------------------------------

// Textures, the emulator, mods, game patches and the roster: the content system. The Play page checks
// on start and installs when the game is closed; this is where a person looks, checks, repairs, or
// installs now.
function contentBlock() {
  const c = state.content;
  let s;
  if (!ok(c)) s = section("Game content", null, el("p", { class: "partial", text: c?.error || "Content status is unavailable. Refresh settings to try again." }));
  else {
    const working = ["checking", "fetching", "installing"].includes(c.phase);
    const labels = { checking: "Checking for updates", fetching: "Downloading content", installing: "Installing content", available: "Update available", staged: "Ready to install", installed: "Content updated", problem: "Needs attention" };
    let action;
    if (c.phase === "available") action = el("button", { type: "button", class: "btn primary", text: `Download ${mb(c.download_bytes)} MB`, disabled: busy, onclick: () => act("Download content", "content_fetch", {}, () => {}) });
    else if (c.phase === "staged") action = el("button", { type: "button", class: "btn primary", text: "Install now", disabled: busy, onclick: () => act("Install content", "content_promote", { confirm: true }, () => {}) });
    else if (c.phase === "installed") action = el("button", { type: "button", class: "btn", text: "Dismiss summary", disabled: busy, onclick: () => act("Dismiss", "content_dismiss", {}, () => {}) });
    else action = el("button", { type: "button", class: "btn", text: working ? "Working…" : "Check game content", disabled: busy || working, onclick: () => act("Check content", "content_check", { depth: null }, () => {}) });
    const details = (c.components || []).filter((x) => x && ["available", "refused", "problem"].includes(x.state));
    s = section("Game content", null,
      row(labels[c.phase] || "Game content updates", { control: action, help: c.note || "Keep your textures, game patches, roster, and Dolphin up to date." }),
      working ? el("progress", { class: "settings-progress", "aria-label": labels[c.phase], ...(c.phase === "fetching" && c.download_bytes ? { value: String(Math.min(c.fetched_bytes || 0, c.download_bytes)), max: String(c.download_bytes) } : {}) }) : null,
      details.length ? el("ul", { class: "pref-list" }, ...details.map((comp) => el("li", { text: comp.state === "available" ? `${comp.component}: ${comp.repair ? "repair" : `${comp.installed ? comp.installed + " → " : ""}${comp.offered}`}, ${mb(comp.bytes)} MB` : `${comp.component}: ${comp.message || comp.state}` }))) : null,
      row("Verify game files", { control: el("button", { type: "button", class: "btn", text: "Verify and repair", disabled: busy || working, onclick: () => act("Verify content", "content_check", { depth: "verify" }, () => {}) }), help: "Check installed content and prepare repairs for missing or changed files." }),
      // The image itself, not the components: walks the game to the newest build and re-applies the
      // roster. 09-26: a player's game-patch component was current while his image was still an old
      // build, so his online copy never matched anyone's and he had no way to fix it from here.
      row("Repair game files", { control: el("button", { type: "button", class: "btn", text: "Repair game files", disabled: busy || working, onclick: () => act("Repair game files", "content_repair_image", {}, (r) => { problem = r?.note || null; }) }), help: "Bring the game image to the newest build and re-apply the roster, if an update left it behind." }));
  }
  s.id = "settings-game-content";
  return s;
}

function updatesBlock() {
  const parts = [];
  if (ok(state.policy)) {
    const automatic = draft.value("updates", state.policy.automatic);
    const policy = el("select", { id: "update-policy", disabled: busy },
      el("option", { value: "automatic", text: "Keep me up to date" }), el("option", { value: "ask", text: "Ask me first" }));
    policy.value = automatic ? "automatic" : "ask";
    policy.addEventListener("change", () => {
      const next = policy.value === "automatic";
      stage("updates", "update-policy", { label: "Automatic updates", value: next, same: next === state.policy.automatic,
        save: async (v) => { state.policy = await call("update_policy_set", { automatic: v }); } });
    });
    parts.push(section("Update preferences", null, row("Automatic updates", { id: "update-policy", control: policy,
      value: automatic ? el("span", { class: "pref-state", text: "Recommended" }) : null,
      help: automatic ? "Download and install verified updates automatically. See a summary when installation is complete." : "Nothing is downloaded until you say so." })));
  } else parts.push(section("Update preferences", null, el("p", { class: "partial", text: state.policy?.error || "Your update preference is unavailable." })));
  const actions = [el("button", { type: "button", class: "btn", text: "Check for updates", disabled: busy, onclick: () => act("Check for updates", "update_check", { source: null }, (r) => { update = r; downloaded = null; }) })];
  if (downloaded?.path) actions.push(el("button", { type: "button", class: "btn primary", text: "Install and restart", disabled: busy, onclick: () => act("Install update", "update_apply", { path: downloaded.path, confirm: true }, (r) => { update = { ...update, note: r.note }; }) }));
  else if (update?.state === "available") actions.push(el("button", { type: "button", class: "btn primary", text: `Download ${update.latest}`, disabled: busy, onclick: () => act("Download update", "update_download", { source: null }, (r) => { downloaded = r; if (r.check) update = r.check; }) }));
  // What changed lives on the site, not in a paragraph here.
  actions.push(el("button", { type: "button", class: "btn quiet", text: "Patch notes", onclick: () => call("open_website", { url: PATCH_NOTES_URL }).catch(() => {}) }));
  // Two presses, on purpose: one press on a pad, or a stray Enter, must not remove the launcher. The
  // first arms the button for a few seconds and says so; the second does it.
  const uninstall = el("button", { type: "button", class: "btn", text: "Uninstall…", disabled: busy, onclick: () => {
    if (uninstall.dataset.armed) { act("Uninstall", "uninstall_launcher", { confirm: true }, () => {}); return; }
    uninstall.dataset.armed = "true"; uninstall.textContent = "Press again to remove";
    setTimeout(() => { if (!uninstall.isConnected) return; delete uninstall.dataset.armed; uninstall.textContent = "Uninstall…"; }, 8000);
  } });
  const version = versionLine(state.paths);
  parts.push(section("Launcher", null,
    version ? row("Version", { value: `NFL Street Reborn ${version}` }) : null,
    row(downloaded?.path ? "Ready to install" : update?.state === "available" ? `Version ${update.latest} available` : "Launcher updates", { control: actions,
      help: downloaded?.path ? `Downloaded and verified · ${mb(downloaded.bytes)} MB. Restart the launcher to install.` : update?.note || "Check for a new launcher version. Your games and saves are preserved." }),
    row("Remove NFL Street Reborn", { control: uninstall, help: "Removes the launcher and the game from this PC. Your saves stay." })));
  parts.push(contentBlock());
  return parts;
}

// ---- Files & storage ---------------------------------------------------------------------------------

const BOXES = [
  { key: "game", setting: "game", name: "Game image", hint: "Your game image. Reset to find it automatically." },
  { key: "legacyRoot", setting: "legacy_root", name: "Existing install", hint: "Your earlier Reborn installation, containing the User and MODS folders." },
  { key: "runtime", setting: "runtime", name: "Emulator", hint: "The bundled Reborn Dolphin is selected automatically." },
  { key: "userDir", setting: "user_dir", name: "User folder", hint: "Contains your saves and Dolphin settings. Reset to use the detected folder." },
];

// Where each location resolves when nothing is chosen, from the commands that know it.
function detected(key) {
  if (key === "game") return ok(state.check) ? state.check.game : null;
  if (key === "runtime") return ok(state.check) ? state.check.runtime : null;
  if (key === "userDir") return ok(state.dolphin) ? state.dolphin.user_dir : null;
  return null;
}
function savedPath(key) { return (ok(state.preferences) ? state.preferences[key] : get(key)) || ""; }

// Label | field (value + Auto while nothing is chosen) | Change… (spec §8.2). Saving goes through the bar.
function pathRow(b) {
  const key = "path." + b.key, id = `path-${b.key}`, saved = savedPath(b.key);
  const value = draft.value(key, saved);
  const entry = (v) => ({ label: b.name, value: v, same: v === saved,
    save: async (next) => {
      const r = await call("settings_set_path", { which: b.setting, value: next });
      // Nothing was stored: the image is NFL Street 2 but not a Reborn build. The choice is put to the
      // player (Repair first), and this change stays pending until they answer.
      if (r && r.unverified) { unverified = { ...r.unverified, which: b.setting, key: b.key, value: next }; return HELD; }
      unverified = null;
      set(b.key, next);
    } });
  const stagePath = (v, focusId = `change-${b.key}`) => stage(key, focusId, entry(v));
  const change = async () => {
    if (busy) return;
    try {
      const picked = await call("pick_path", { kind: b.key === "game" ? "game" : b.key === "runtime" ? "runtime" : "folder" });
      if (picked) stagePath(picked);
    } catch (e) { problem = reason(e); draw(); }
  };
  // Editable, so a path can be pasted (spec §10). It shows what is in use; typing over it stages a choice
  // and clearing it goes back to automatic.
  const found = value ? "" : detected(b.key) || "";
  const shown = value || found;
  // "Auto" only when something was actually found; with nothing chosen and nothing found the box is empty.
  const input = el("input", { id, type: "text", value: shown, placeholder: "Not set", spellcheck: "false", disabled: busy, "aria-describedby": found ? `${id}-auto` : null });
  typedPath(input, shown, (v, loud) => {
    if (loud) stagePath(v, id);
    else { draft.put(key, entry(v)); lastChangedId = id; refreshBar(); }
  });
  return row(b.name, { id, cls: "path-entry",
    value: el("div", { class: "field" },
      input,
      found ? el("span", { class: "auto", id: `${id}-auto`, text: "Auto" }) : null),
    control: [
      el("button", { type: "button", id: `change-${b.key}`, class: "btn", text: "Change…", "aria-label": `Change ${b.name}`, disabled: busy, onclick: change }),
      value ? el("button", { type: "button", class: "btn quiet sm", text: "Reset", "aria-label": `Reset ${b.name} to automatic`, disabled: busy, onclick: () => stagePath("") }) : null,
    ],
    help: b.hint });
}

// The repair every refusal points at: the same download the setup wizard offers, on the page the Play
// badge and the launch refusal send the player to.
async function repairGame() {
  await act("Repair my game", "setup_download_reborn_game", {}, () => { unverified = null; draft.drop("path.game"); refreshBar(); });
}

// "Play it anyway": the escape hatch, taken deliberately, for this one image.
// This is the ONLY call in the launcher that sets acknowledgeUnverified.
async function playItAnyway(choice) {
  await act(choice.play_anyway, "settings_set_path", { which: choice.which, value: choice.value, acknowledgeUnverified: true }, () => {
    unverified = null;
    set(choice.key, choice.value);
    draft.drop("path." + choice.key);
    refreshBar();
  });
}

// The Tier 2 choice, drawn as a modal. Repair is primary; the escape hatch is not. Cancel leaves the
// change pending (Discard drops it).
function unverifiedChoice() {
  const choice = unverified;
  if (!choice) return null;
  const dialog = el("dialog", { id: "unverified-game", class: "settings-choice", role: "alertdialog", "aria-labelledby": "unverified-game-title" },
    el("h2", { id: "unverified-game-title", text: choice.headline }),
    el("p", { text: choice.detail }),
    el("div", { class: "settings-actions" },
      el("button", { type: "button", class: "btn quiet", text: "Cancel", disabled: busy, onclick: () => { unverified = null; draw(); } }),
      el("button", { type: "button", class: "btn", text: choice.play_anyway, disabled: busy, onclick: () => playItAnyway(choice) }),
      el("button", { type: "button", class: "btn primary", "data-primary": "true", text: choice.repair, disabled: busy, onclick: () => repairGame() })));
  // Esc, or B through the shell (cancel then close), is Cancel: the choice is over (so the next draw does
  // not reopen it) and the change stays pending. No redraw here. A dialog a redraw removed is not an answer.
  dialog.addEventListener("close", () => { if (dialog.isConnected && unverified === choice) unverified = null; });
  return dialog;
}

// The standing repair, for a player whose saved game file is already an unconverted disc.
function repairBlock() {
  if (!ok(state.check) || state.check.game_verdict !== "unconverted") return null;
  const s = section("Your game file isn't NFL Street Reborn.", "This is NFL Street 2, but not a Reborn build, so rosters, faces and stats won't match. This usually means setup couldn't convert your disc.",
    row("Repair", { control: el("button", { type: "button", class: "btn primary", text: "Repair my game", disabled: busy, onclick: () => repairGame() }) }));
  s.id = "repair-game";
  return s;
}

function foundBlock() {
  const c = state.check;
  if (!ok(c)) return section("Detected files", null, partial(`Could not check: ${c ? c.error : "not loaded"}`));
  if (!c.ready) return section("Detected files", null, refusal(c.problem || "Your installation needs attention before you can play."));
  const release = state.installation?.installed_release?.release;
  return section("Detected files", null, ...(release ? [row("Previous release", { value: release })] : []), row("Emulator", { value: c.runtime }), row("Game file", { value: c.game }));
}

function whereBlock() {
  const p = state.paths;
  if (!ok(p)) return section("Storage", null, partial(`Could not read: ${p ? p.error : "not loaded"}`));
  const dirs = Array.isArray(p.directories) ? p.directories : [];
  const present = dirs.filter((d) => d.exists).map((d) => d.relative);
  return section("Storage", null,
    row("Channel", { value: p.channel_note || p.channel }),
    row("Program files", { value: p.install_root }),
    row("Your data", { value: p.data_root }),
    row("Save data", { value: p.saves_survive_uninstall ? "Kept separately from the application." : "Stored inside the application folder." }),
    row("Folders in use", { value: present.length ? present.join(", ") : "No data folders exist yet." }));
}

function filesBlock() {
  return [repairBlock(), unverifiedChoice(),
    section("Game & installation", "Found automatically. Choose a different location only to override it.", ...BOXES.map(pathRow),
      row("Game setup", { control: el("a", { href: "onboarding.html", class: "btn", text: "Resume game setup" }) })),
    foundBlock(), whereBlock()];
}

// ---- the page ------------------------------------------------------------------------------------------

function pickTab(name) {
  if (busy) return;
  settingsTab = name;
  draw();
  document.querySelector?.('.tabs .tab[aria-pressed="true"]')?.focus({ preventScroll: true });
}

function draw() {
  const screen = document.getElementById("screen"), parts = [];
  if (problem) parts.push(refusal(problem));
  parts.push(dolphinCard());
  parts.push(tabs("settings", TABS, settingsTab, pickTab, { disabled: busy, label: "Settings sections" }));
  const body = settingsTab === "Updates" ? updatesBlock() : settingsTab === "Graphics" ? graphicsBlock()
    : settingsTab === "Files & storage" ? filesBlock() : settingsTab === "Display" ? [lookBlock(), dolphinBlock()] : [dolphinBlock()];
  parts.push(...body);
  parts.push(el("button", { type: "button", class: "btn quiet sm refresh", text: "Refresh settings", disabled: busy, onclick: refresh }));
  // repairBlock and unverifiedChoice draw nothing most of the time, and a null handed to
  // replaceChildren becomes the word "null" on the page.
  screen.replaceChildren(...parts.filter(Boolean));
  // A real browser hides a <dialog> until it is opened.
  if (unverified) document.getElementById("unverified-game")?.showModal?.();
}

async function refresh() {
  problem = null;
  try {
    await load();
  } catch (e) {
    problem = reason(e);
  }
  draw();
}

// Dolphin can close while this page stays open, including when it was opened outside the launcher.
// Refresh availability without rebuilding controls on every poll or overwriting a slider in use.
let pollingDolphin = false;
async function refreshDolphinAvailability() {
  if (pollingDolphin || busy || document.hidden || ["Updates", "Files & storage"].includes(settingsTab) || !state.dolphin) return;
  pollingDolphin = true;
  try {
    const next = await call("dolphin_preferences", { runtime: get("runtime") });
    if (busy || !next || ["Updates", "Files & storage"].includes(settingsTab)) return;
    const availability = (value) => JSON.stringify([value.editable, value.live_editable, value.readonly_reason]);
    const changed = availability(next) !== availability(state.dolphin);
    state.dolphin = next;
    if (changed) { const focused = document.activeElement?.id; draw(); if (focused) document.getElementById(focused)?.focus({ preventScroll: true }); }
  } catch (_) { /* A failed read is not evidence that Dolphin stopped. */ }
  finally { pollingDolphin = false; }
}
document.addEventListener("session-state", refreshDolphinAvailability);
window.addEventListener("focus", refreshDolphinAvailability);

// A content check or download can outlive the page that started it. Update only its section so the
// preference being edited keeps its focus.
let pollingContent = false;
setInterval(async () => {
  if (pollingContent || busy || settingsTab !== "Updates" || document.hidden || !["checking", "fetching", "installing"].includes(state.content?.phase)) return;
  pollingContent = true;
  try {
    const next = await call("content_status");
    if (busy || settingsTab !== "Updates") return;
    if (JSON.stringify(next) !== JSON.stringify(state.content)) {
      state.content = next;
      document.getElementById("settings-game-content")?.replaceWith(contentBlock());
    }
  } catch (e) { problem = reason(e); }
  finally { pollingContent = false; }
}, 2000);

const pill = document.getElementById("bridge");
if (pill) {
  pill.textContent = bridge ? "Launcher connected" : "Preview";
  pill.className = "pill " + (bridge ? "ok" : "bad");
}
markCurrent();
wireCoreFold();
loadLook();
if (bridge) {
  refresh();
} else {
  problem = "The launcher core is not attached. Open this page inside the launcher window.";
  draw();
}
