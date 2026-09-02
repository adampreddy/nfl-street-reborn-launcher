// Settings, as a page: where things live, and the few paths a person can set.

import { bridge, call, cleanPath, el, fact, facts, get, markCurrent, partial, reason, refusal, set, wireCoreFold } from "./shell.js";

const state = {};
let problem = null;

function ok(x) {
  return x && !x.error;
}

async function load() {
  const reads = {
    paths: call("paths"),
    check: call("launch_check", { runtime: get("runtime"), game: get("game") }),
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
    name: "Emulator",
    hint: "Leave blank on Windows: the emulator that came with the launcher is the only one that works, and it is found by itself. On macOS or Linux, a path to a Dolphin here is allowed and is used as is.",
  },
  {
    key: "userDir",
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
        refresh();
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

function draw() {
  const screen = document.getElementById("screen");
  const parts = [];
  if (problem) parts.push(refusal(problem));
  parts.push(boxesBlock(), foundBlock(), whereBlock());
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
