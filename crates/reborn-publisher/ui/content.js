// Textures, runtime, mods: publish a folder as the next version of one
// thing players get. One button: "Publish textures 1.2.1".
//
// This file gathers the inputs (which thing, the folder, the version, and
// a game id for textures) into the shape `content_plan` / `content_run` /
// `content_verify` expect, and hands them to `job.js`, which drives the
// check, the plain-words card, the real publish and the read-back. Every
// number on the card comes straight off the plan value `content.rs`
// returned; nothing here computes a publishing rule.
//
// The runtime check is a small block of its own: a "Check this folder"
// button that calls `content_lint_runtime` directly, so a folder the
// launcher has already started can be shown before the button is pressed.
// The check and the publish run the identical test server-side regardless.

import { call, el, refusal, partial, fmtBytes, markCurrent, thingWord } from "./shell.js";
import { job } from "./job.js";

markCurrent();

const screen = document.getElementById("screen");

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

// The three things this page publishes. The roster, the launcher and the
// game patch each have their own page.
const COMPONENTS = [
  ["textures", "Textures"],
  ["runtime", "Runtime"],
  ["mods", "Mods"],
];

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

/// The last dot-separated number plus one. Only a starting point to edit.
function proposeVersion(current) {
  if (!current) return "1.0.0";
  const parts = String(current).split(".");
  const last = parts.length - 1;
  const n = Number(parts[last]);
  if (!Number.isFinite(n)) return `${current}.1`;
  parts[last] = String(n + 1);
  return parts.join(".");
}

// -- inputs ---------------------------------------------------------------

const componentSelect = el(
  "select",
  { id: "ct-component" },
  ...COMPONENTS.map(([value, label]) => el("option", { value, text: label }))
);
const folderInput = el("input", { type: "text", placeholder: "folder to publish from", id: "ct-folder", style: "min-width:360px" });
const versionInput = el("input", { type: "text", placeholder: "version", id: "ct-version", style: "width:120px" });
const gameIdSelect = el(
  "select",
  { id: "ct-game-id" },
  el("option", { value: "GN7E69", text: "GN7E69 (offline)" }),
  el("option", { value: "FR4N31", text: "FR4N31 (online)" })
);
const gameIdLabel = el("label", { class: "inline" }, "Game id", gameIdSelect);
gameIdLabel.hidden = componentSelect.value !== "textures";

const versionNote = el("p", { class: "hint" });
let currentVersion = null;

function selection() {
  return {
    component: componentSelect.value,
    folder: folderInput.value.trim(),
    version: versionInput.value.trim(),
    game_id: componentSelect.value === "textures" ? gameIdSelect.value : null,
  };
}

function remember() {
  call("publisher_set_setting", {
    key: "remembered.content",
    value: { component: componentSelect.value, folder: folderInput.value.trim(), game_id: gameIdSelect.value },
  });
}

async function loadVersionInfo() {
  gameIdLabel.hidden = componentSelect.value !== "textures";
  const component = componentSelect.value;
  const word = thingWord(component);
  const answer = await call("history_versions", { component });
  if (!answer.ok) {
    versionNote.replaceChildren(refusal(`Existing versions could not be read: ${answer.error}`));
    theJob.refresh();
    return;
  }
  const { current_version: current, versions } = answer.value || {};
  currentVersion = current || null;
  const proposed = proposeVersion(current);
  if (!versionInput.value.trim()) versionInput.value = proposed;
  const known = new Set((Array.isArray(versions) ? versions : []).map((v) => v.version));
  const parts = [];
  parts.push(current ? `Players have ${word} ${current} now.` : `Players do not get ${word} from the server yet.`);
  parts.push(`Next: ${proposed}.`);
  const chosen = versionInput.value.trim();
  if (chosen && known.has(chosen)) parts.push(`${chosen} was already published; choose a version that has not been used.`);
  versionNote.replaceChildren(partial(parts.join(" ")));
  theJob.refresh();
}

componentSelect.addEventListener("change", () => {
  versionInput.value = "";
  lintResult = null;
  renderLint();
  loadVersionInfo();
  remember();
});
folderInput.addEventListener("change", () => {
  remember();
  renderLint();
});
gameIdSelect.addEventListener("change", remember);
versionInput.addEventListener("change", loadVersionInfo);
versionInput.addEventListener("input", () => theJob.refresh());

// -- the runtime check, on its own ------------------------------------------

const lintHost = el("div");
let lintResult = null;
let lintChecking = false;

function lintOffendingList(offending) {
  return (offending || []).map((o) => `${o.path} (${o.why})`).join("; ");
}

function renderLint() {
  if (componentSelect.value !== "runtime") {
    lintHost.replaceChildren();
    return;
  }
  const children = [
    el(
      "div",
      { class: "row" },
      el("button", { text: "Check this folder", disabled: lintChecking, onclick: onCheckRuntime }),
      el("span", { class: "hint", text: "A runtime the launcher has already started (logs, a User folder) is refused." })
    ),
  ];
  if (lintChecking) {
    children.push(partial("Checking..."));
  } else if (lintResult) {
    if (!lintResult.ok) children.push(refusal(lintResult.error));
    else if (lintResult.value.clean) children.push(el("p", { class: "said", text: lintResult.value.message }));
    else children.push(refusal(`${lintResult.value.message} Found: ${lintOffendingList(lintResult.value.offending)}`));
  }
  lintHost.replaceChildren(...children);
}

async function onCheckRuntime() {
  const folder = folderInput.value.trim();
  if (!folder) {
    lintResult = { ok: false, error: "Choose a runtime folder to check first." };
    renderLint();
    return;
  }
  lintChecking = true;
  renderLint();
  lintResult = await call("content_lint_runtime", { folder });
  lintChecking = false;
  renderLint();
}

// -- the job ------------------------------------------------------------------

const jobHost = el("div");

const theJob = job(jobHost, {
  operation: "content",
  verb: () => {
    const s = selection();
    if (!s.folder || !s.version) return "";
    return `Publish ${thingWord(s.component)} ${s.version}`;
  },
  plan: () => {
    remember();
    return call("content_plan", selection());
  },
  run: (dryRun) => call("content_run", { ...selection(), dry_run: dryRun }),
  verify: () => call("content_verify", { component: componentSelect.value }),
  describePlan: (plan) => {
    if (!plan || typeof plan !== "object" || !("component" in plan)) return [plan && plan.summary ? String(plan.summary) : "The check came back with nothing to describe."];
    const word = thingWord(plan.component);
    const lines = [`${plan.files} file(s), ${fmtBytes(plan.bytes)}, published as ${word} ${plan.version}.`];
    lines.push(`${plan.new_blobs} file(s) (${fmtBytes(plan.new_bytes)}) are new to the server; ${plan.shared_blobs} are already there.`);
    if (typeof plan.player_fetch_bytes === "number") {
      const who = currentVersion ? `Players on ${word} ${currentVersion}` : "Players";
      lines.push(`${who} will download ${fmtBytes(plan.player_fetch_bytes)} (${plan.player_fetch_files} file(s)).`);
    } else if (plan.player_fetch_note) {
      lines.push(String(plan.player_fetch_note));
    }
    return lines;
  },
  describeDone: (state) => {
    const plan = state && state.plan && typeof state.plan === "object" ? state.plan : {};
    const s = selection();
    return `Done. Players now get ${thingWord(plan.component || s.component)} ${plan.version || s.version}.`;
  },
  confirmLabel: "Yes, publish",
  emptyLabel: "Choose a folder and a version first",
  onSettled: () => loadVersionInfo(),
});

screen.append(
  block(
    "Publish a folder",
    el(
      "div",
      { class: "row" },
      el("label", { class: "inline" }, "What", componentSelect),
      el("label", { class: "inline" }, "Folder", folderInput),
      browseFolderButton(folderInput, () => {
        remember();
        renderLint();
        theJob.refresh();
      }),
      el("label", { class: "inline" }, "Version", versionInput),
      gameIdLabel
    ),
    versionNote,
    lintHost,
    jobHost
  )
);

// -- startup ------------------------------------------------------------------

async function loadRemembered() {
  const answer = await call("publisher_settings");
  if (!answer.ok || !answer.value || !answer.value.remembered) return;
  const saved = answer.value.remembered.content;
  if (!saved || typeof saved !== "object") return;
  if (typeof saved.component === "string" && COMPONENTS.some(([value]) => value === saved.component)) componentSelect.value = saved.component;
  if (typeof saved.folder === "string") folderInput.value = saved.folder;
  if (typeof saved.game_id === "string") gameIdSelect.value = saved.game_id;
}

async function init() {
  await loadRemembered();
  renderLint();
  await loadVersionInfo();
}

init();
