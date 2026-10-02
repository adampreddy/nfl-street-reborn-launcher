// A focused Play screen. Launch checks remain visible; settings and paths
// live in Game details without moving the artwork or interrupting navigation.

import { bridge, call, el, fact, facts, get, icon, markCurrent, minutes, reason, set, wireCoreFold } from "./shell.js";
import { openMenu } from "./menu.js";
import { avatarSrc } from "./live-common.js";

// Opened from a friend's invite link (a Netplay `join/` or a lobby `room/`):
// straight on to Online, which fills in the room or offers Join. Play is the
// window's first page, so this is where the hand-off lives; the code stays
// put until Online has used it.
async function handOffInvite() {
  try {
    const pending = await call("netplay_pending_join");
    const room = pending && pending.code ? null : await call("mm_pending_room").catch(() => null);
    if (!(pending && pending.code) && !(room && room.code)) return;
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
// One line about a strip change that did not apply (Display, Graphics). Never the red "could not
// start": that state is for the game itself.
let notice = null;
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

// Names only (spec §8.1): the ReShade text they used to carry is stale since 3.0.20.
const PRESETS = [
  ["performance", "Performance"],
  ["balanced", "Balanced"],
  ["quality", "All-Out"],
];

const GLYPH_PLAY = "M7 4l13 8-13 8z";
const GLYPH_MORE = "M5 14a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm7 0a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm7 0a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z";
const CHEVRON = "M9 6l6 6-6 6";

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
    mods: call("mod_packages"),
    controls: call("controls_current"),
    home: call("home_state", { runtime: get("runtime"), legacyRoot: get("legacyRoot") }),
    progress: call("onboarding_progress", { legacyRoot: get("legacyRoot") }),
    paths: call("paths"),
    identity: call("identity_status"),
  };
  // The device scan starts the runtime and takes about 1.5 s (everything else answers in milliseconds), so Play
  // never waits for it: it draws with the last scan (or "Checking…") and redraws when the fresh one arrives.
  if (state.devices === undefined) state.devices = cachedDevices();
  const scan = call("devices", { runtime: get("runtime") });
  scan.then((v) => { state.devices = v; cacheDevices(v); draw(); }, (e) => { state.devices = { error: reason(e) }; draw(); });
  const keys = Object.keys(reads);
  const settled = await Promise.allSettled(Object.values(reads));
  keys.forEach((k, i) => {
    state[k] = settled[i].status === "fulfilled" ? settled[i].value : { error: reason(settled[i].reason) };
  });
  loadAvatar();
}

// The Discord picture for the account chip: identity_status has none, the player's profile does. Drawn from the
// last answer at once, refreshed in the background; any failure keeps the letter.
const AVATAR_KEY = "reborn:avatar";
function cachedAvatar(playerId) { try { const v = JSON.parse(localStorage.getItem(AVATAR_KEY) || "null"); return v && v.id === playerId ? avatarSrc({ avatar_url: v.url }) : null; } catch (_) { return null; } }
function avatarPic(name) {
  if (!state.avatar) return el("span", { class: "avatar", "aria-hidden": "true", text: name.slice(0, 1).toUpperCase() });
  const img = el("img", { class: "avatar", src: state.avatar, alt: "", "aria-hidden": "true" });
  img.addEventListener?.("error", () => { state.avatar = null; draw(); });
  return img;
}
function loadAvatar() {
  const id = state.identity;
  if (!ok(id) || !id.signed_in || !id.player_id) { state.avatar = null; return; }
  state.avatar = cachedAvatar(id.player_id);
  call("player_profile", { playerId: id.player_id }).then((p) => {
    const url = avatarSrc({ avatar_url: p?.header?.avatar_url });
    try { localStorage.setItem(AVATAR_KEY, JSON.stringify({ id: id.player_id, url })); } catch (_) {}
    if (url !== state.avatar) { state.avatar = url; draw(); }
  }, () => {});
}

const DEVICES_KEY = "reborn:devices";
function cachedDevices() { try { const v = JSON.parse(localStorage.getItem(DEVICES_KEY) || "null"); return v && Array.isArray(v.pads) ? v : undefined; } catch (_) { return undefined; } }
function cacheDevices(v) { try { if (v && Array.isArray(v.pads)) localStorage.setItem(DEVICES_KEY, JSON.stringify(v)); } catch (_) {} }

async function play() {
  if (busy || externalSession) return;
  busy = true;
  playing = true;
  result = null;
  problem = null;
  notice = null;
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

// Top right (spec §8.1): avatar, name and playtime, in place of "Signed in as …". The avatar is the Discord
// picture (loadAvatar) or, until it arrives or when there is none, the name's first letter (spec §3.4).
function accountBlock() {
  const id = state.identity;
  const box = el("div", { class: "play-account", role: "group", "aria-label": "Account" });
  if (signIn.active) {
    // The address, in every case, not only when the browser did not open: it can open on another
    // monitor, behind this window, or in a profile not signed into Discord.
    box.append(el("div", { class: "glass play-account-card", role: "status" },
      el("p", { text: `Waiting for Discord. Your code is ${signIn.userCode}.` }),
      signIn.verifyUrl ? el("p", { text: `Go to ${signIn.verifyUrl} to approve it.` }) : null,
      el("button", { type: "button", class: "btn sm", text: "Cancel", onclick: () => { stopSignIn(); draw(); } })));
  } else if (ok(id) && id.signed_in) {
    const name = id.display_name || "Player";
    const played = ok(state.home) && state.home.sessions_played ? `${minutes(state.home.minutes_played)} played` : "No games played yet";
    box.append(el("a", { class: "play-account-chip", href: "stats.html", "aria-label": `Signed in as ${name}` },
      avatarPic(name),
      el("span", { class: "play-account-text" }, el("span", { class: "nm", text: name }), el("span", { class: "pt", text: played }))));
  } else {
    box.append(el("button", { type: "button", class: "btn sm", text: "Sign in with Discord", disabled: busy, onclick: startSignIn }));
    // Never `id.error`: a failed read hands back the command's own name or a vault's wording.
    box.append(el("p", { class: "play-account-note", text: ok(id) ? "Sign in with Discord to track your stats from your games." : ACCOUNT_UNREADABLE }));
  }
  // In every state: the note written for a signed-out player (their game is waiting for sign-in).
  if (result?.stats_note) box.append(el("p", { class: "play-account-note", text: result.stats_note }));
  if (signIn.message) box.append(el("p", { class: "play-account-note", text: signIn.message }));
  return box;
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
  if (d === undefined) return { text: "Checking…", note: null };
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
  // The player's own test for a right pack is "can I see stuff, and what version is it": read off the
  // folder the launch will use. It moved here from the Play screen (decluttered, spec §8.1).
  const pack = ok(check) ? check.pack : null;
  rows.push(fact("Texture pack", pack ? (pack.reachable ? (pack.version || "loaded") : "not loading") : null));
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
    el("button", { type: "button", class: "btn sm", text: "Close", onclick: () => dialog.close() })));
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
    const copy = el("button", { type: "button", class: "btn sm play-copy", text: "Copy error", onclick: async () => {
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
    el("button", { type: "button", class: "btn", text: "Open Dolphin settings", disabled: busy || externalSession, onclick: () => { dialog.close(); openDolphin(); } })));
  dialog.addEventListener("close", () => document.getElementById("game-details-open")?.focus());
  dialog.addEventListener("click", event => { if (event.target === dialog) { const r = dialog.getBoundingClientRect(); if (event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom) dialog.close(); } });
  return dialog;
}

// The one line under the button (spec §8.1): five states, in the words Play has always used. A setup
// that is not finished is "Setup required", never "could not start"; the reason is in Game details.
function playStatus(ready, inSession, error) {
  const crashed = !inSession && !error && result?.needs_recovery;
  const [tone, text] = inSession ? ["live", "Enjoy the game"]
    : error ? ["error", "Your game could not start"]
    : crashed ? ["error", "Game closed unexpectedly"]
    : !ready ? ["setup", "Setup required"]
    : ["ready", "Ready to play"];
  const action = tone === "setup" ? el("a", { class: "play-status-action", href: "onboarding.html", text: "Finish setup" })
    : tone === "error" ? el("button", { type: "button", class: "play-status-action", text: "See why", onclick: openDetails })
    : null;
  return el("div", { class: `play-status is-${tone}`, role: tone === "error" ? "alert" : "status" },
    el("span", { class: "dot", "aria-hidden": "true" }), el("span", { class: "play-state-title", text }), action);
}

// Things the player must act on before, or instead of, playing. Quiet otherwise (empty = hidden).
function notes() {
  const box = el("div", { class: "play-notes" });
  // Play was NOT started because player one's pad cannot press Start or A. Not a failure: a setup to offer.
  if (controllerSetup) box.append(el("div", { class: "glass play-note play-controller-setup", role: "alert" },
    el("strong", { text: "Your controller needs a one-time setup" }),
    el("p", { text: controllerSetup.message }),
    el("div", { class: "play-controller-setup-actions" },
      el("a", { class: "btn primary sm", href: "controls.html", text: "Set up controller" }),
      el("button", { type: "button", id: "play-anyway", class: "btn sm", disabled: busy, text: "Play anyway", onclick: () => { playAnyway = true; play(); } }))));
  // Tier 2 of the game-image law: a disc this launcher could not verify as a Reborn build. Always a
  // fault to repair, never a preference.
  if (state.check?.game_verdict === "unconverted") box.append(el("div", { class: "glass play-note play-warning", role: "status" },
    el("span", { class: "dot", "aria-hidden": "true" }), el("span", { text: "Not a Reborn build" }),
    el("a", { class: "play-warning-repair", href: "settings.html#files", text: "Repair" })));
  if (state.check?.pack && !state.check.pack.reachable) box.append(el("div", { class: "glass play-note play-warning", role: "status" },
    el("span", { class: "dot", "aria-hidden": "true" }), el("span", { text: "Texture pack not loading" })));
  if (notice) box.append(el("div", { class: "glass play-note", role: "status" }, el("span", { text: notice })));
  return box;
}

function cell(tag, attrs, key, value) {
  return el(tag, { class: "play-cell", ...attrs },
    el("span", {}, el("span", { class: "k", text: key }), el("span", { class: "v", text: value })),
    icon(CHEVRON));
}

// The strip (spec §8.1): Controllers, Graphics, Display. Graphics and Display open a small menu
// (names only); no checkboxes, switches or segmented controls in the strip.
function strip(inSession) {
  const pads = ok(state.devices) && Array.isArray(state.devices.pads) ? state.devices.pads : null;
  const dolphin = ok(state.dolphin) ? state.dolphin : null;
  const fullscreen = String(dolphin?.values?.fullscreen ?? "True").toLowerCase() === "true";
  const displayLocked = busy || inSession || !dolphin || dolphin.editable === false;
  return el("nav", { class: "glass play-strip", "aria-label": "Game preferences" },
    cell("a", { href: "controls.html" }, "Controllers", pads ? pads.length ? `${pads.length} connected` : "Connect a controller" : "Manage devices"),
    cell("button", { type: "button", id: "play-graphics", "aria-haspopup": "menu", "aria-expanded": "false", disabled: busy || inSession,
      onclick: (e) => openMenu(e.currentTarget, { label: "Graphics", options: PRESETS.map(([value, label]) => ({ value, label })),
        // A saved custom or previous-install setting is not one of the three: nothing is checked.
        current: ok(state.graphicsOptions) && state.graphicsOptions.current ? null : preset(), onPick: pickGraphics }) },
      "Graphics", graphicsLabel()),
    // While a Display write is out the cell stays focusable (a pad keeps its place; app-shell skips
    // disabled and aria-disabled controls) but does not open.
    cell("button", { type: "button", id: "play-display", "aria-haspopup": "menu", "aria-expanded": "false", disabled: displayLocked,
      ...(displaySaving ? { "aria-busy": "true" } : {}),
      onclick: (e) => { if (!displaySaving) openMenu(e.currentTarget, { label: "Display", options: [{ value: "true", label: "Fullscreen" }, { value: "false", label: "Windowed" }],
        current: String(fullscreen), onPick: pickDisplay }); } },
      "Display", fullscreen ? "Fullscreen" : "Windowed"));
}

// The same write Settings' graphics preset has always made; the preset name is kept where Play reads it.
async function pickGraphics(value) {
  if (busy) return;
  notice = null;
  try {
    await call("setup_save_graphics_options", { mode: "preset", resolution: 3, msaa: 1, ssaa: false });
    set("preset", value);
    if (ok(state.graphicsOptions)) state.graphicsOptions = { ...state.graphicsOptions, current: null };
  } catch (e) { notice = `Graphics: ${reason(e)}`; }
  draw();
}

// One Display write at a time (the old checkbox disabled itself while its write was out): a quick
// second pick is dropped, never sent twice. Picking the value already set sends nothing, as a
// checkbox could not "change" to its own state.
let displaySaving = false;

async function pickDisplay(value) {
  const dolphin = ok(state.dolphin) ? state.dolphin : null;
  if (!dolphin || busy || displaySaving) return;
  if (String(dolphin.values?.fullscreen ?? "True").toLowerCase() === value) return;
  displaySaving = true;
  notice = null;
  draw();
  try {
    await call("dolphin_preference_set", { runtime: get("runtime"), key: "fullscreen", value });
    dolphin.values = { ...(dolphin.values || {}), fullscreen: value };
  } catch (e) { notice = `Display: ${reason(e)}`; }
  finally { displaySaving = false; }
  draw();
}

// The launch group rises in on the page's first draw only (ruling P2-6): every later draw (Play, a pick,
// a refresh, a session change) rebuilds the same elements, and replaying the animation would make them jump.
let entered = false;

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
  const scene = el("section", { class: "play-scene", "aria-hidden": "true" },
    el("div", { class: "hero-art" }, el("img", { src: "art/play-background-display.jpg", alt: "" })));
  // The artwork element itself survives every redraw, so it never flickers or moves.
  const art = screen.querySelector?.(".hero-art");
  if (art) scene.querySelector(".hero-art").replaceWith(art);
  const error = problem || state.check?.error || null;
  const actions = el("div", { class: "play-actions" });
  if (ready || inSession) actions.append(el("button", { type: "button", id: "play-now", "data-primary": "true", class: "btn primary play", disabled: busy || inSession, onclick: play },
    icon(GLYPH_PLAY, { fill: true }), el("span", { text: inSession ? "Playing" : busy ? "Opening…" : "Play now" })));
  else actions.append(el("a", { href: "onboarding.html", class: "btn primary play", "data-primary": "true", text: "Set up your game" }));
  actions.append(el("button", { type: "button", id: "game-details-open", class: "btn play-more", title: "Game details", "aria-label": "Game details", "aria-haspopup": "dialog", onclick: openDetails },
    icon(GLYPH_MORE, { fill: true })));
  const content = el("div", { class: "play-content" },
    el("h1", { class: "play-wordmark" }, el("img", { src: "art/play-logo-display.png", alt: "NFL Street Reborn" })),
    actions, playStatus(ready, inSession, error), notes());
  const dialog = detailsDialog();
  screen.classList?.toggle("is-entering", !entered);
  entered = true;
  screen.replaceChildren(scene, accountBlock(), content, strip(inSession), dialog);
  if (wasOpen) { dialog.showModal(); if (filesOpen) dialog.querySelector("details").open = true; }
  else if (focusedId) { const target = document.getElementById(focusedId); if (target && !target.disabled) target.focus({ preventScroll: true }); }
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
