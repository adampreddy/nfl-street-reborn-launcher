// Settings, as a page: where things live, and the few paths a person can set.

import { bridge, call, cleanPath, el, fact, facts, get, markCurrent, partial, reason, refusal, set, wireCoreFold } from "./shell.js";

const state = {};
let problem = null;
let busy = false;
// The update flow's last answers, shown in place.
let update = null; // update_check view
let downloaded = null; // update_download view
let leagueSaid = null;

function ok(x) {
  return x && !x.error;
}

async function load() {
  const reads = {
    preferences: call("settings_paths"),
    installation: call("setup_install_info"),
    paths: call("paths"),
    check: call("launch_check", { runtime: get("runtime"), game: get("game"), userDir: get("userDir") }),
    token: call("credential_stored", { account: "nsr.league.ingest" }),
    policy: call("update_policy"),
    queue: call("stats_queue"),
  };
  const keys = Object.keys(reads);
  const settled = await Promise.allSettled(Object.values(reads));
  keys.forEach((k, i) => {
    state[k] = settled[i].status === "fulfilled" ? settled[i].value : { error: reason(settled[i].reason) };
  });
}

// The boxes. Saved as typed, quotes stripped, and the Play page re-checks on
// its next load. Blank means "find it", which is the normal state.
const BOXES = [
  {
    key: "game",
    setting: "game",
    name: "Game image",
    hint: "Your game image. Leave empty to find it automatically.",
  },
  {
    key: "legacyRoot",
    setting: "legacy_root",
    name: "Existing install",
    hint: "Your earlier Reborn installation, containing the User and MODS folders.",
  },
  {
    key: "runtime",
    setting: "runtime",
    name: "Emulator",
    hint: "The bundled Reborn Dolphin is selected automatically on Windows.",
  },
  {
    key: "userDir",
    setting: "user_dir",
    name: "User folder",
    hint: "Contains your saves and Dolphin settings. Leave empty to use the detected folder.",
  },
];

function boxesBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Game & installation" }));
  sec.append(el("p", { class: "note", text: "Locations are detected automatically. Choose an override only if you want to use a different installation." }));
  for (const b of BOXES) {
    const value = ok(state.preferences) ? state.preferences[b.key] : get(b.key);
    const input = el("input", { id: `path-${b.key}`, type: "text", class: "wide", value: value || "", placeholder: "Detect automatically", disabled: busy });
    const save = async () => {
      const v = cleanPath(input.value);
      await act(`Save ${b.name}`, "settings_set_path", { which: b.setting, value: v }, () => { set(b.key, v); });
    };
    const browse = async () => {
      if (busy) return;
      try {
        const path = await call("pick_path", { kind: b.key === "game" ? "game" : b.key === "runtime" ? "runtime" : "folder" });
        if (path) { input.value = path; await save(); }
      } catch (e) { problem = reason(e); draw(); }
    };
    const row = el("div", { class: "path-entry" }, input,
      el("button", { type: "button", text: "Browse", "aria-label": `Browse for ${b.name}`, disabled: busy, onclick: browse }),
      el("button", { type: "button", text: "Save", "aria-label": `Save ${b.name}`, disabled: busy, onclick: save }));
    sec.append(el("div", { class: "settings-row stack" }, el("label", { for: `path-${b.key}`, text: b.name }), row, el("span", { class: "hint", text: b.hint })));
  }
  sec.append(el("a", { href: "onboarding.html", class: "button", text: "Resume game setup" }));
  return sec;
}

function foundBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Detected files" }));
  const c = state.check;
  if (!ok(c)) {
    sec.append(partial(`Could not check: ${c ? c.error : "not loaded"}`));
    return sec;
  }
  if (!c.ready) {
    sec.append(refusal(c.problem || "Your installation needs attention before you can play."));
    return sec;
  }
  const release=state.installation?.installed_release?.release;
  sec.append(facts([...(release?[fact("Previous release", release)]:[]),fact("Emulator", c.runtime),fact("Game image", c.game)]));
  return sec;
}

function whereBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Storage" }));
  const p = state.paths;
  if (!ok(p)) {
    sec.append(partial(`Could not read: ${p ? p.error : "not loaded"}`));
    return sec;
  }
  sec.append(
    facts([
      fact("Channel", p.channel_note || p.channel),
      fact("Program files", p.install_root),
      fact("Your data", p.data_root),
      fact("Save data", p.saves_survive_uninstall ? "Kept separately from the application." : "Stored inside the application folder."),
    ]),
  );
  const dirs = Array.isArray(p.directories) ? p.directories : [];
  const present = dirs.filter((d) => d.exists).map((d) => d.relative);
  sec.append(el("p", { class: "note", text: present.length ? `Folders in use under your data: ${present.join(", ")}.` : "No data folders exist yet." }));
  return sec;
}

async function act(label, cmd, args, after) {
  if (busy) return;
  busy = true;
  problem = null;
  if (window.setLauncherBusy) window.setLauncherBusy(true); else draw();
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
}

function updatesBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Updates" }));
  sec.append(el("p", { class: "note", text: "Check for a release, download it, then install when you are ready. Your saves stay separate from the launcher." }));
  // Whether it does this on its own. Asked in first run, changed here.
  const pol = state.policy;
  if (ok(pol)) {
    for (const [auto, what, why] of [
      [true, "Keep Me Up To Date", "Download and install verified updates automatically."],
      [false, "Ask me first", "Nothing is downloaded until you say so."],
    ]) {
      const input = el("input", { type: "radio", name: "policy", checked: pol.automatic === auto, disabled: busy, onchange: () => act("Update setting", "update_policy_set", { automatic: auto }, () => {}) });
      sec.append(el("label", { class: "choice" + (pol.automatic === auto ? " selected" : "") }, input, el("span", { class: "what", text: what }), el("span", { class: "why", text: why })));
    }
    sec.append(el("p", { class: "note", text: "Use Check for updates below to check this installation now." }));
  } else {
    sec.append(partial(`The update setting could not be read: ${pol ? pol.error : "not loaded"}`));
  }

  const row = el("div", { class: "mod" });
  row.append(el("button", { type: "button", text: "Check for updates", disabled: busy, onclick: () => act("Check", "update_check", { source: null }, (r) => { update = r; downloaded = null; }) }));
  if (update && update.state === "available") {
    row.append(el("button", { type: "button", text: `Download ${update.latest}`, disabled: busy, onclick: () => act("Download", "update_download", { source: null }, (r) => { downloaded = r; }) }));
  }
  if (downloaded && downloaded.path) {
    row.append(el("button", { type: "button", class: "write", text: "Install and restart", disabled: busy, onclick: () => act("Install", "update_apply", { path: downloaded.path, confirm: true }, (r) => { problem = null; leagueSaid = null; update = { ...update, note: r.note }; }) }));
  }
  sec.append(row);
  if (update) {
    sec.append(el("p", { class: update.state === "unreachable" || update.state === "unreadable" ? "partial" : "", text: update.note }));
    if (update.notes) sec.append(el("p", { class: "note", text: update.notes }));
  } else {
    sec.append(el("p", { class: "absent", text: "Not checked yet." }));
  }
  if (downloaded && downloaded.path) {
    sec.append(el("div", { class: "said" }, el("p", { text: `Downloaded and verified: ${downloaded.path} (${(downloaded.bytes / 1048576).toFixed(1)} MB).` })));
  }
  return sec;
}

function leagueBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "League results" }));
  sec.append(el("p", { class: "note", text: "Send your recorded match results to the league. Your access token is stored securely in Windows." }));
  const t = state.token;
  const q = state.queue;
  const rows = [];
  rows.push(fact("Token", ok(t) ? (t.stored ? "stored" : "not stored") : null));
  rows.push(fact("Waiting to send", ok(q) ? String(q.queued ?? q.pending ?? 0) : null));
  sec.append(facts(rows));
  const input = el("input", { type: "password", class: "wide", id: "league-token", placeholder: "Enter your league token", autocomplete: "off" });
  const row = el("div", { class: "settings-row stack" }, el("label", { for: "league-token", class: "name", text: "League token" }), input);
  const buttons = el("div", { class: "mod" });
  buttons.append(el("button", { type: "button", text: "Store token", disabled: busy, onclick: () => { const v = input.value; input.value = ""; act("Store token", "stats_credential_set", { token: v }, (r) => { leagueSaid = "League token saved."; }); } }));
  buttons.append(el("button", { type: "button", text: "Forget token", disabled: busy || !(ok(t) && t.stored), onclick: () => act("Forget token", "stats_credential_forget", {}, () => { leagueSaid = "Token removed."; }) }));
  buttons.append(el("button", { type: "button", class: "write", text: "Send results now", disabled: busy || !(ok(t) && t.stored), onclick: () => act("Send", "stats_submit", { confirm: true }, (r) => { leagueSaid = r.note; }) }));
  sec.append(row, buttons);
  if (leagueSaid) sec.append(el("div", { class: "said" }, el("p", { text: leagueSaid })));
  return sec;
}

function draw() {
  const screen = document.getElementById("screen");
  const parts = [];
  if (problem) parts.push(refusal(problem));
  const advanced = el("details", { class: "block" }, el("summary", { text: "League & storage" }), leagueBlock(), whereBlock());
  parts.push(boxesBlock(), foundBlock(), updatesBlock(), advanced);
  screen.replaceChildren(...parts);
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
