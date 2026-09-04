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
    paths: call("paths"),
    check: call("launch_check", { runtime: get("runtime"), game: get("game") }),
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
    hint: "Full path to the .iso. Blank: the image in your existing NFL Street Reborn install is used.",
  },
  {
    key: "legacyRoot",
    name: "Existing install",
    hint: "The folder your NFL Street Reborn 2.1 lives in, the one holding User and MODS. This is where the game image, the saves and the mods come from.",
  },
  {
    key: "runtime",
    setting: "runtime",
    name: "Emulator",
    hint: "Leave blank on Windows: the emulator that came with the launcher is the only one that works, and it is found by itself. On macOS or Linux, a path to a Dolphin here is allowed and is used as is.",
  },
  {
    key: "userDir",
    setting: "user_dir",
    name: "User folder",
    hint: "Blank: the folder that already holds your saves is found and used. Set this only to play against a different one.",
  },
];

function boxesBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Paths you can set" }));
  for (const b of BOXES) {
    const input = el("input", {
      type: "text",
      class: "wide",
      value: get(b.key) || "",
      placeholder: "blank: found by itself",
      onchange: (ev) => {
        const v = cleanPath(ev.target.value);
        ev.target.value = v;
        set(b.key, v);
        // Also on disk, where the Play and files shortcuts can read it.
        // Kept only in this window, a path set here would be ignored by
        // every shortcut and nothing would say so.
        if (b.setting) {
          act(`Save ${b.name}`, "settings_set_path", { which: b.setting, value: v }, () => {});
        } else {
          refresh();
        }
      },
    });
    sec.append(el("div", { class: "settings-row stack" }, el("span", { class: "name", text: b.name }), input, el("span", { class: "hint", text: b.hint })));
  }
  return sec;
}

function foundBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "What those paths resolve to" }));
  const c = state.check;
  if (!ok(c)) {
    sec.append(partial(`Could not check: ${c ? c.error : "not loaded"}`));
    return sec;
  }
  if (!c.ready) {
    sec.append(refusal(c.problem || "A launch would refuse."));
    return sec;
  }
  sec.append(facts([fact("Emulator", c.runtime), fact("", c.runtime_note), fact("Game image", c.game)]));
  return sec;
}

function whereBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Where the launcher keeps things" }));
  const p = state.paths;
  if (!ok(p)) {
    sec.append(partial(`Could not read: ${p ? p.error : "not loaded"}`));
    return sec;
  }
  sec.append(
    facts([
      fact("Channel", `${p.channel}. ${p.channel_note || ""}`.trim()),
      fact("Program files", p.install_root),
      fact("Your data", p.data_root),
      fact("Saves survive uninstall", p.saves_survive_uninstall ? "yes: the data folder is outside the program folder" : "NO: the data folder is inside the program folder"),
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
  draw();
  try {
    const r = await call(cmd, args);
    after(r);
  } catch (e) {
    problem = `${label}: ${reason(e)}`;
  }
  await load();
  busy = false;
  draw();
}

function updatesBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Updates" }));
  sec.append(el("p", { class: "note", text: "Three steps, each its own button, so nothing installs behind your back: check the release page, download and verify the installer, then run it. The installer replaces the program files and leaves your data and saves alone." }));
  // Whether it does this on its own. Asked in first run, changed here.
  const pol = state.policy;
  if (ok(pol)) {
    for (const [auto, what, why] of [
      [true, "Update on its own", "Downloaded in the background and applied before you next play."],
      [false, "Ask me first", "Nothing is downloaded until you say so."],
    ]) {
      const input = el("input", { type: "radio", name: "policy", checked: pol.automatic === auto, disabled: busy, onchange: () => act("Update setting", "update_policy_set", { automatic: auto }, () => {}) });
      sec.append(el("label", { class: "choice" + (pol.automatic === auto ? " selected" : "") }, input, el("span", { class: "what", text: what }), el("span", { class: "why", text: why })));
    }
    sec.append(el("p", { class: "note", text: pol.note }));
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
  sec.append(el("p", { class: "note", text: "Match results the game recorded wait here until they are sent to the league site. Sending needs the league token, kept in Windows' own credential store and never shown again." }));
  const t = state.token;
  const q = state.queue;
  const rows = [];
  rows.push(fact("Token", ok(t) ? (t.stored ? "stored" : "not stored") : null));
  rows.push(fact("Waiting to send", ok(q) ? String(q.queued ?? q.pending ?? 0) : null));
  sec.append(facts(rows));
  const input = el("input", { type: "password", class: "wide", placeholder: "league token", autocomplete: "off" });
  const row = el("div", { class: "settings-row stack" }, el("span", { class: "name", text: "League token" }), input);
  const buttons = el("div", { class: "mod" });
  buttons.append(el("button", { type: "button", text: "Store token", disabled: busy, onclick: () => { const v = input.value; input.value = ""; act("Store token", "stats_credential_set", { token: v }, (r) => { leagueSaid = `Stored a token of ${r.length} characters.`; }); } }));
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
  parts.push(boxesBlock(), foundBlock(), updatesBlock(), leagueBlock(), whereBlock());
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
  pill.textContent = bridge ? "core attached" : "core not attached";
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
