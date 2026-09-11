// Full installer set: build what a brand new player downloads, upload it,
// and point the website at it. One button: "Build and publish installer
// 3.0.0".
//
// `job.js` drives the check (measures the four inputs, reads what the
// website points at now), the card, the real build (about twenty minutes,
// its output streaming into Details), the upload, and the read-back of the
// download pointer. Every number on the card comes straight off the plan
// value `set.rs` returned.

import { call, el, partial, fmtBytes, markCurrent } from "./shell.js";
import { job } from "./job.js";

markCurrent();

const screen = document.getElementById("screen");

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

function browseFolderButton(input, onChosen) {
  return el("button", {
    text: "Browse",
    onclick: async () => {
      const answer = await call("pick_folder", {});
      if (answer.ok && answer.value) {
        input.value = answer.value;
        if (onChosen) onChosen();
      }
    },
  });
}

// -- inputs -----------------------------------------------------------------

const bundleInput = el("input", { type: "text", placeholder: String.raw`C:\Reborn Origin-stage\game-import-bundle-fields-12-19`, style: "min-width:420px" });
const runtimeInput = el("input", { type: "text", placeholder: "the Reborn Dolphin runtime folder", style: "min-width:420px" });
const texturesInput = el("input", { type: "text", placeholder: "the pristine texture tree", style: "min-width:420px" });
const promptPackagesInput = el("input", { type: "text", placeholder: "the prompt packages folder", style: "min-width:420px" });
const versionInput = el("input", { type: "text", placeholder: "e.g. 3.0.0", style: "width:140px" });

function selection() {
  return {
    bundle: bundleInput.value.trim(),
    runtime: runtimeInput.value.trim(),
    textures: texturesInput.value.trim(),
    prompt_packages: promptPackagesInput.value.trim(),
    version: versionInput.value.trim(),
  };
}

function remember() {
  call("publisher_set_setting", { key: "remembered.set", value: selection() });
  theJob.refresh();
}

for (const input of [bundleInput, runtimeInput, texturesInput, promptPackagesInput, versionInput]) {
  input.addEventListener("change", remember);
  input.addEventListener("input", () => theJob.refresh());
}

const inputsBlock = block(
  "Inputs",
  el("p", { class: "hint", text: "Remembered from the last run. The game-import bundle must not sit inside this repository's own target folder; the build empties that folder first." }),
  el("div", { class: "row" }, el("label", { class: "inline" }, "Game-import bundle", bundleInput), browseFolderButton(bundleInput, remember)),
  el("div", { class: "row" }, el("label", { class: "inline" }, "Runtime folder", runtimeInput), browseFolderButton(runtimeInput, remember)),
  el("div", { class: "row" }, el("label", { class: "inline" }, "Texture folder", texturesInput), browseFolderButton(texturesInput, remember)),
  el("div", { class: "row" }, el("label", { class: "inline" }, "Prompt packages", promptPackagesInput), browseFolderButton(promptPackagesInput, remember)),
  el("div", { class: "row", style: "margin-top:10px" }, el("label", { class: "inline" }, "Version", versionInput))
);

// -- the job ----------------------------------------------------------------

const jobHost = el("div");

function fmtCount(n) {
  return typeof n === "number" && Number.isFinite(n) ? String(n) : "an unknown number of";
}

const theJob = job(jobHost, {
  operation: "set",
  verb: () => (versionInput.value.trim() ? `Build and publish installer ${versionInput.value.trim()}` : ""),
  plan: () => call("set_plan", selection()),
  run: (dryRun) => call("set_run", { ...selection(), dry_run: dryRun }),
  verify: () => call("set_verify"),
  describePlan: (plan) => {
    if (!plan || typeof plan !== "object" || !("version" in plan)) return [plan && plan.summary ? String(plan.summary) : "The check came back with nothing to describe."];
    const lines = [];
    if (plan.warning) lines.push(`Warning: ${plan.warning}`);
    const hosted = plan.hosted || {};
    if (hosted.reachable === false) lines.push(`Warning: what the website points at now could not be read: ${hosted.why || "unknown reason"}.`);
    else if (!hosted.version) lines.push("Nothing has been published for new players yet; this would be the first.");
    else lines.push(`New players download installer ${hosted.version} now (${fmtBytes(hosted.bytes)}).`);
    const inputs = plan.inputs || {};
    const parts = ["bundle", "runtime", "textures", "prompt_packages"]
      .map((k) => inputs[k])
      .filter((e) => e && typeof e === "object")
      .map((e) => `${fmtCount(e.files)} file(s), ${fmtBytes(e.bytes)}`);
    lines.push(`This builds installer ${plan.version} from four folders (${fmtBytes(plan.total_bytes)} in all${parts.length ? `: ${parts.join("; ")}` : ""}), uploads the zip, then points the website's download at it.`);
    if (plan.builder && !plan.builder.found) lines.push(`Warning: the Reborn Builder is not at ${plan.builder.path}; the build will refuse until it is.`);
    lines.push("The build takes about twenty minutes.");
    return lines;
  },
  describeDone: (state) => {
    const plan = state && state.plan && typeof state.plan === "object" ? state.plan : {};
    return `Done. New players now download installer ${plan.version || versionInput.value.trim()}.`;
  },
  confirmLabel: "Yes, build and publish",
  emptyLabel: "Type a version first",
});

// -- startup ------------------------------------------------------------------

async function loadRemembered() {
  const answer = await call("publisher_settings");
  if (!answer.ok || !answer.value || !answer.value.remembered) return;
  const saved = answer.value.remembered.set;
  if (!saved || typeof saved !== "object") return;
  if (typeof saved.bundle === "string" && saved.bundle) bundleInput.value = saved.bundle;
  if (typeof saved.runtime === "string") runtimeInput.value = saved.runtime;
  if (typeof saved.textures === "string") texturesInput.value = saved.textures;
  if (typeof saved.prompt_packages === "string") promptPackagesInput.value = saved.prompt_packages;
  if (typeof saved.version === "string") versionInput.value = saved.version;
}

screen.replaceChildren(inputsBlock, block("Build, upload, and point the website at it", jobHost));

async function init() {
  await loadRemembered();
  if (!bundleInput.value.trim()) bundleInput.value = String.raw`C:\Reborn Origin-stage\game-import-bundle-fields-12-19`;
  theJob.refresh();
}

init();
