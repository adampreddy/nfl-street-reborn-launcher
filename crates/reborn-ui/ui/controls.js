// Controls, as a page: the button art setting, and the pads the emulator sees.

import { bridge, call, el, fact, facts, get, markCurrent, partial, reason, refusal, wireCoreFold } from "./shell.js";

const state = {};
let busy = false;
let problem = null;
let notice = null;
let lastSync = null;

function ok(x) {
  return x && !x.error;
}

async function load() {
  const reads = {
    current: call("controls_current"),
    options: call("controls_options", { runtime: get("runtime") }),
    devices: call("devices", { runtime: get("runtime") }),
  };
  const keys = Object.keys(reads);
  const settled = await Promise.allSettled(Object.values(reads));
  keys.forEach((k, i) => {
    state[k] = settled[i].status === "fulfilled" ? settled[i].value : { error: reason(settled[i].reason) };
  });
}

async function choose(slug) {
  if (busy) return;
  busy = true;
  problem = null;
  notice = null;
  draw();
  try {
    const r = await call("controls_set", { profile: slug });
    if (r && r.synced) lastSync = r.synced;
    else if (r) notice = r.sync_note;
    if (r && r.missing_package) notice = `${notice ? notice + " " : ""}The ${r.label} pack (${r.missing_package}) is not installed, so the game draws its own art until it is.`;
  } catch (e) {
    problem = reason(e);
  }
  await load();
  busy = false;
  draw();
}

function artBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Button art" }));
  const c = state.current;
  if (ok(c) && c.profile) {
    sec.append(el("p", { text: `Current: ${c.label}, from ${c.source === "settings" ? "Settings" : "first run"}.` }));
  } else if (ok(c)) {
    sec.append(el("p", { class: "absent", text: "Nothing chosen yet. The game draws its own art until one is picked." }));
  } else {
    sec.append(partial(`Which art is in force could not be read: ${c ? c.error : "not loaded"}`));
  }
  const o = state.options;
  if (!ok(o)) {
    sec.append(partial(`The list of profiles could not be read: ${o ? o.error : "not loaded"}`));
    return sec;
  }
  for (const opt of Array.isArray(o) ? o : []) {
    const isCurrent = !!(ok(c) && c.profile === opt.slug);
    const input = el("input", { type: "radio", name: "art", checked: isCurrent, disabled: busy, onchange: () => choose(opt.slug) });
    const what = opt.label + (opt.suggested ? "  (your pad suggests this)" : "");
    const why = (opt.describes || "") + (opt.pack ? ` Applies ${opt.pack}.` : " Applies no pack: the disc's own art.");
    sec.append(el("label", { class: "choice" + (isCurrent ? " selected" : "") }, input, el("span", { class: "what", text: what }), el("span", { class: "why", text: why })));
  }
  if (lastSync) {
    sec.append(el("div", { class: "said" }, el("p", { text: lastSync.note || "" })));
  }
  return sec;
}

function padsBlock() {
  const sec = el("section", { class: "block" }, el("h2", { text: "Pads the emulator can see" }));
  const d = state.devices;
  if (!ok(d)) {
    sec.append(partial(`Pads could not be listed: ${d ? d.error : "not loaded"}`));
    return sec;
  }
  const pads = Array.isArray(d.pads) ? d.pads : [];
  const other = Array.isArray(d.other) ? d.other : [];
  if (pads.length === 0) {
    sec.append(el("p", { class: "absent", text: "No pad found. A wireless pad that went to sleep counts as none; wake it and reload this page." }));
  }
  const rows = [];
  for (const p of pads) {
    rows.push(fact(p.port ? `Port ${p.port}` : "Unassigned", `${p.device_string} (${p.family})${p.set_aside ? `, set aside: ${p.set_aside}` : ""}`));
  }
  for (const p of other) {
    rows.push(fact("Not a pad", `${p.device_string} (${p.family})`));
  }
  if (rows.length) sec.append(facts(rows));
  const how = d.source === "runtime" ? "asked the emulator itself" : `from ${d.source || "the built-in scan"}`;
  sec.append(el("p", { class: "note", text: `Listed ${how}. ${d.complete ? "Every backend was searched." : `Not every backend was searched${(d.unsearched || []).length ? ` (${d.unsearched.join(", ")})` : ""}, so a pad could be missing from this list.`}` }));
  return sec;
}

function draw() {
  const screen = document.getElementById("screen");
  const parts = [];
  if (problem) parts.push(refusal(problem));
  if (notice) parts.push(partial(notice));
  parts.push(artBlock(), padsBlock());
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
