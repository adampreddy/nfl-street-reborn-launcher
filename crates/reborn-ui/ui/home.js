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

const PRESETS = [
  ["performance", "Performance", "Native resolution. For a weak or hot machine."],
  ["balanced", "Balanced", "Twice the resolution. The usual choice."],
  ["quality", "Quality", "Three times the resolution."],
  ["ultra", "Ultra", "Four times. Needs a strong graphics card."],
];

function preset() {
  const chosen = get("preset");
  if (chosen) return chosen;
  const s = state.progress && state.progress.summary;
  return (s && s.graphics) || "balanced";
}

async function load() {
  const reads = {
    check: call("launch_check", { runtime: get("runtime"), game: get("game") }),
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
  if (busy) return;
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
    });
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

function playBlock() {
  const sec = el("section", { class: "block" });
  const check = state.check;
  const ready = ok(check) && check.ready;

  const button = el("button", {
    type: "button",
    class: "play",
    text: playing ? "Game running" : "Play Now",
    disabled: busy || !ready,
    onclick: play,
  });
  sec.append(button);

  if (playing) {
    sec.append(el("p", { class: "note", text: "The game is running. This page waits for it to close, then says how it went." }));
  }
  if (problem) sec.append(refusal(problem));
  if (check && check.error) sec.append(refusal(`Could not check what a launch would use: ${check.error}`));
  if (ok(check) && !check.ready) {
    sec.append(refusal(check.problem || "A launch would refuse."));
    sec.append(el("p", { class: "note", text: "Set a path on the Settings page, or point it at your existing NFL Street Reborn install." }));
  }

  if (result) {
    const said = el("div", { class: "said" });
    said.append(el("p", { text: result.message || "" }));
    if (result.mods_note) said.append(el("p", { text: result.mods_note }));
    if (result.stats_note) said.append(el("p", { text: result.stats_note }));
    if (result.user_dir_note) said.append(el("p", { text: result.user_dir_note }));
    if (Array.isArray(result.pads) && result.pads.length) said.append(el("p", { text: result.pads.join(". ") + "." }));
    sec.append(said);
    if (result.needs_recovery) {
      sec.append(partial("That session ended badly. The Advanced page's sessions command has the detail; a recovery screen is not built yet."));
    }
  }

  const progress = state.progress;
  if (ok(progress) && progress.complete === false) {
    const p = el("p", { class: "partial" });
    p.append("First run has not finished, so some of the answers below are still absent. ");
    p.append(el("a", { href: "onboarding.html", text: "Go to First run." }));
    sec.append(p);
  }
  return sec;
}

function graphicsBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Graphics" }));
  sec.append(el("p", { class: "note", text: "Picked here, used by the next Play. First run's answer is the starting point." }));
  const current = preset();
  for (const [slug, label, why] of PRESETS) {
    const input = el("input", {
      type: "radio",
      name: "preset",
      checked: slug === current,
      disabled: busy,
      onchange: () => {
        set("preset", slug);
        draw();
      },
    });
    sec.append(el("label", { class: "choice" + (slug === current ? " selected" : "") }, input, el("span", { class: "what", text: label }), el("span", { class: "why", text: why })));
  }
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
  rows.push(fact("Graphics", PRESETS.find((p) => p[0] === preset())?.[1] || preset()));
  rows.push(fact("Channel", ok(paths) ? paths.channel : null));
  if (ok(home)) {
    rows.push(
      fact(
        "Played",
        home.sessions_played
          ? `${home.sessions_played} session(s), ${minutes(home.minutes_played)}, last ${home.last_played || "unknown"}`
          : "never",
      ),
    );
  } else {
    rows.push(fact("Played", null));
  }
  sec.append(facts(rows));
  return sec;
}

function draw() {
  const screen = document.getElementById("screen");
  screen.replaceChildren(playBlock(), graphicsBlock(), factsBlock());
}

async function refresh() {
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
