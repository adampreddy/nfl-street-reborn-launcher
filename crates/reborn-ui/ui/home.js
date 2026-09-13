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
    slots: call("controller_slots"),
    controlOptions: call("controls_options", {runtime:null}),
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
async function openDetails() {
  const dialog=document.getElementById("game-details");dialog.showModal();
  try{state.mods=await call('mod_packages');dialog.querySelector('.play-mods')?.replaceWith(quickMods());}catch{}
}

function quickMods() {
  const group=el('fieldset',{class:'play-mods'},el('legend',{text:'Mods'}));
  const managed=state.mods?.managed_by_controls||[];
  const packages=(state.mods?.packages||[]).filter(p=>!managed.includes(p.id)&&/football|soundtrack|music|audio/i.test(`${p.name} ${p.id}`));
  const status=el('p',{class:'play-mods-status',role:'status'});
  const choices=el('div',{class:'play-mods-choices'});
  for(const p of packages){
    const checkbox=el('input',{type:'checkbox',checked:!!p.active,disabled:busy||externalSession,'data-mod':p.id});
    checkbox.addEventListener('change',async()=>{
      const on=checkbox.checked;choices.querySelectorAll('input').forEach(n=>n.disabled=true);status.textContent='Saving mod selection…';
      let saved=false;
      try{
        const result=await call('mods_toggle',{id:p.id,version:p.version,on,runtime:get('runtime')});
        saved=true;state.mods=await call('mod_packages');
        for(const input of choices.querySelectorAll('input'))input.checked=!!state.mods.packages?.find(mod=>mod.id===input.dataset.mod)?.active;
        status.textContent=result?.synced===false?result.note||'Selection saved. The mod files could not be applied yet.':'Saved. Applies to your next game.';
        status.setAttribute('role','status');
      }catch(e){
        try{state.mods=await call('mod_packages');for(const input of choices.querySelectorAll('input'))input.checked=!!state.mods.packages?.find(mod=>mod.id===input.dataset.mod)?.active;}
        catch{if(!saved)checkbox.checked=!on;}
        status.textContent=saved?'Saved, but the selection could not be refreshed. Reopen Game details to check it.':reason(e);status.setAttribute('role','alert');
      }
      finally{choices.querySelectorAll('input').forEach(n=>n.disabled=busy||externalSession);}
    });
    choices.append(el('label',{},el('span',{text:p.name||p.id}),checkbox));
  }
  if(!packages.length)choices.append(el('p',{class:'note',text:state.mods?.error?'Your mods could not be loaded. Reopen Game details to try again.':'No football or soundtrack mods installed.'}));
  group.append(choices,status);return group;
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
  ]) links.append(el("a", { href }, el("span", { text: title }), el("span", { class: "play-detail-value", text: value }), el("span", { text: "›", "aria-hidden": "true" })));
  dialog.append(links, quickMods());
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
