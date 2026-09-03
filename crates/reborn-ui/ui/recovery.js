// Recovery, as a page: the last session as recorded, and six ways forward.
//
// The six come from the design's recovery screen: relaunch, start with mods
// disabled, start with Performance graphics, open the emulator itself,
// verify and repair, and a diagnostic bundle. Nothing restarts by itself.

import { bridge, call, el, fact, facts, get, markCurrent, minutes, partial, reason, refusal, wireCoreFold } from "./shell.js";

const state = {};
let busy = false;
let playing = false;
let problem = null;
let said = null;

function ok(x) {
  return x && !x.error;
}

async function load() {
  const reads = {
    sessions: call("sessions"),
    check: call("launch_check", { runtime: get("runtime"), game: get("game") }),
  };
  const keys = Object.keys(reads);
  const settled = await Promise.allSettled(Object.values(reads));
  keys.forEach((k, i) => {
    state[k] = settled[i].status === "fulfilled" ? settled[i].value : { error: reason(settled[i].reason) };
  });
}

async function act(label, cmd, args, opts = {}) {
  if (busy) return;
  busy = true;
  problem = null;
  said = null;
  playing = !!opts.launch;
  draw();
  try {
    const r = await call(cmd, args);
    said = opts.say ? opts.say(r) : JSON.stringify(r);
  } catch (e) {
    problem = `${label}: ${reason(e)}`;
  }
  playing = false;
  await load();
  busy = false;
  draw();
}

function launchArgs(extra) {
  return {
    runtime: get("runtime"),
    game: get("game"),
    preset: get("preset") || "balanced",
    fullscreen: true,
    userDir: get("userDir"),
    modsDisabled: false,
    ...extra,
  };
}

function launchSaid(r) {
  return [r.message, r.mods_note, r.stats_note].filter(Boolean).join(" ");
}

function lastSessionBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Last session" }));
  const s = state.sessions;
  if (!ok(s)) {
    sec.append(partial(`Sessions could not be read: ${s ? s.error : "not loaded"}`));
    return sec;
  }
  const list = Array.isArray(s) ? s : [];
  if (list.length === 0) {
    sec.append(el("p", { class: "absent", text: "No session has been recorded yet." }));
    return sec;
  }
  const last = list[list.length - 1];
  const bad = list.filter((x) => x.needs_recovery);
  sec.append(
    facts([
      fact("Ended", last.ended),
      fact("Lasted", minutes(last.minutes)),
      fact("Build", last.build),
      fact("Needs recovery", last.needs_recovery ? "yes" : "no, it ended normally"),
      fact("Bad endings on record", bad.length ? `${bad.length} of ${list.length}` : "none"),
    ]),
  );
  if (bad.length >= 3) {
    sec.append(partial("Three or more sessions have ended badly. Start with mods disabled first: if that runs clean, a mod is the likely cause."));
  }
  return sec;
}

function actionsBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Ways forward" }));
  const check = state.check;
  const ready = ok(check) && check.ready;
  if (ok(check) && !ready) sec.append(refusal(check.problem || "A launch would refuse."));

  const rows = [
    ["Relaunch the game", "The same launch as Play.", () => act("Relaunch", "launch", launchArgs({}), { launch: true, say: launchSaid }), ready],
    ["Start with mods disabled", "One launch on the untouched textures. Your mod selection stays as it is for next time.", () => act("Mods disabled", "launch", launchArgs({ modsDisabled: true }), { launch: true, say: launchSaid }), ready],
    ["Start with Performance graphics", "Native resolution, this once. Your preset is not changed.", () => act("Performance", "launch", launchArgs({ preset: "performance" }), { launch: true, say: launchSaid }), ready],
    ["Open NFL Street Reborn Dolphin", "The emulator alone, no game, against the same user folder. For its own settings.", () => act("Open Dolphin", "open_runtime", { runtime: get("runtime"), userDir: get("userDir") }, { say: (r) => `Started ${r.runtime}. ${r.user_dir_note}` }), true],
    ["Create a diagnostic bundle", "Copies settings, the emulator's config and log, and the session list into one folder you can send.", () => act("Diagnostic bundle", "diagnostic_bundle", { runtime: get("runtime") }, { say: (r) => `Written to ${r.folder}: ${r.files.join(", ")}.${r.skipped.length ? ` Not found: ${r.skipped.join("; ")}.` : ""}` }), true],
  ];
  for (const [label, why, on, enabled] of rows) {
    const button = el("button", { type: "button", text: label, disabled: busy || !enabled, onclick: on });
    sec.append(el("div", { class: "mod" }, button, el("span", { class: "why", text: why })));
  }
  const verify = el("div", { class: "mod" });
  verify.append(el("span", { class: "what", text: "Verify and repair" }));
  verify.append(el("span", { class: "why", text: "Checks the installed files against their manifest. On the Advanced page as library_verify and library_repair; a page for it is not built yet." }));
  sec.append(verify);

  if (playing) sec.append(el("p", { class: "note", text: "The game is running. This page waits for it to close." }));
  if (said) sec.append(el("div", { class: "said" }, el("p", { text: said })));
  return sec;
}

function draw() {
  const screen = document.getElementById("screen");
  const parts = [];
  if (problem) parts.push(refusal(problem));
  parts.push(lastSessionBlock(), actionsBlock());
  screen.replaceChildren(...parts);
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
