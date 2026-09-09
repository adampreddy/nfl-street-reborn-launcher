// The Content page: publish textures, the runtime, or mods from a folder,
// and the runtime lint that keeps a started runtime from ever going out.
//
// Publishing itself runs through the shared operation panel (`panel.js`),
// the same as History's rollback: this file only gathers the inputs
// (component, folder, version, and a game id for textures) into the shape
// `content_plan`/`content_run`/`content_verify` expect. `panel.js` shows
// one flattened plan summary line; the richer facts the brief for this
// page asks for (file count and total size, the release sequence before
// and after, what a player will fetch) live in `state.plan`, the same
// object `content.rs`'s `content_plan` returned, so this file polls
// `op_status` on its own (same endpoint, same interval `panel.js` polls)
// purely to draw that table, exactly the way `history.js` already does
// for rollback. Nothing here computes a rule; every value drawn is either
// straight off a `content_*`/`history_versions` call or straight off
// `op_status`.
//
// The runtime lint has its own small block, independent of the panel: a
// "Check this folder" button that calls `content_lint_runtime` directly,
// so a dirty folder can be shown before Plan is even clicked. Plan and Run
// run the identical check server-side before writing anything regardless;
// this button is a faster, dedicated answer to the same question.

import { call, el, fact, facts, refusal, partial, fmtBytes, markCurrent } from "./shell.js";
import { panel } from "./panel.js";

markCurrent();

const screen = document.getElementById("screen");

/// A Browse button beside a folder `input`: opens the native picker for
/// a folder and drops the chosen path straight into the field, exactly as
/// if it had been typed. A cancelled dialog leaves the field alone.
function browseFolderButton(input) {
  return el("button", {
    text: "Browse",
    onclick: async () => {
      const answer = await call("pick_folder", {});
      if (answer.ok && answer.value) input.value = answer.value;
    },
  });
}

// The three components this page publishes. Roster, the launcher and the
// game patch each have their own page.
const COMPONENTS = [
  ["textures", "Textures"],
  ["runtime", "Runtime"],
  ["mods", "Mods"],
];

const POLL_MS = 400;

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

/// A patch-bump proposal: the last dot-separated segment, incremented by
/// one. Not full semver, and not meant to be: it only has to give the
/// operator a sensible starting point to edit or accept.
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
const folderInput = el("input", {
  type: "text",
  placeholder: "folder to publish from",
  id: "ct-folder",
  style: "min-width:360px",
});
const versionInput = el("input", { type: "text", placeholder: "version", id: "ct-version" });
const gameIdSelect = el(
  "select",
  { id: "ct-game-id" },
  el("option", { value: "GN7E69", text: "GN7E69 (offline)" }),
  el("option", { value: "FR4N31", text: "FR4N31 (online)" })
);
const gameIdLabel = el("label", { class: "inline" }, "Game id", gameIdSelect);
gameIdLabel.hidden = componentSelect.value !== "textures";

const versionNote = el("p", { class: "hint" });

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
    value: {
      component: componentSelect.value,
      folder: folderInput.value.trim(),
      game_id: gameIdSelect.value,
    },
  });
}

async function loadVersionInfo() {
  gameIdLabel.hidden = componentSelect.value !== "textures";
  const component = componentSelect.value;
  const answer = await call("history_versions", { component });
  if (!answer.ok) {
    versionNote.replaceChildren(refusal(`Existing versions could not be read: ${answer.error}`));
    return;
  }
  const { current_version: current, versions } = answer.value;
  const proposed = proposeVersion(current);
  if (!versionInput.value.trim()) {
    versionInput.value = proposed;
  }
  const known = new Set((versions || []).map((v) => v.version));
  const parts = [];
  parts.push(current ? `Currently live: ${current}.` : `${component} is not part of the currently signed release yet.`);
  parts.push(`Proposed next version: ${proposed}.`);
  const chosen = versionInput.value.trim();
  if (chosen && known.has(chosen)) {
    parts.push(`${chosen} is already published; choose a version the origin does not hold.`);
  }
  versionNote.replaceChildren(partial(parts.join(" ")));
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

// -- the runtime lint, on its own ------------------------------------------

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
      el("span", {
        class: "hint",
        text: "Checks for the marks of a runtime the launcher has already started, before Plan does the same check.",
      })
    ),
  ];
  if (lintChecking) {
    children.push(partial("Checking..."));
  } else if (lintResult) {
    if (!lintResult.ok) {
      children.push(refusal(lintResult.error));
    } else if (lintResult.value.clean) {
      children.push(el("p", { class: "said", text: lintResult.value.message }));
    } else {
      children.push(refusal(`${lintResult.value.message} Found: ${lintOffendingList(lintResult.value.offending)}`));
    }
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

// -- the panel, and its own richer plan facts ------------------------------

const factsHost = el("div");
const panelHost = el("div");

screen.append(
  block(
    "Publish content",
    el(
      "p",
      { class: "hint" },
      "Hashes a folder into the origin as a new version of the chosen component, then re-signs the " +
        "release carrying every other pin forward. A runtime folder is linted first: a runtime the " +
        "launcher has already started is refused before anything is written."
    ),
    el(
      "div",
      { class: "row" },
      el("label", { class: "inline" }, "Component", componentSelect),
      el("label", { class: "inline" }, "Folder", folderInput),
      browseFolderButton(folderInput),
      el("label", { class: "inline" }, "Version", versionInput),
      gameIdLabel
    ),
    versionNote,
    lintHost,
    factsHost,
    panelHost
  )
);

panel(panelHost, {
  operation: "content",
  describe:
    "Plan hashes the folder and shows the file count and total size, the release sequence before and " +
    "after, and what a player already on the previous version would fetch. Writes nothing. Run publishes " +
    "and uploads. Verify reads the hosted release back and confirms the published pin.",
  plan: () => call("content_plan", selection()),
  run: (dryRun) => call("content_run", { ...selection(), dry_run: dryRun }),
  verify: () => call("content_verify", { component: componentSelect.value }),
});

let lastRenderedPlanMarker = null;

function releaseSequenceText(plan) {
  const before = plan.release_sequence_before === null || plan.release_sequence_before === undefined
    ? "none yet"
    : plan.release_sequence_before;
  return `${before} → ${plan.release_sequence_after}`;
}

function renderPlanFacts(plan) {
  if (!plan || typeof plan !== "object" || !("component" in plan)) {
    return;
  }
  const marker = JSON.stringify(plan);
  if (marker === lastRenderedPlanMarker) return;
  lastRenderedPlanMarker = marker;
  factsHost.replaceChildren(
    facts([
      fact("Component", `${plan.component} ${plan.version}`),
      fact("Install root", plan.install_root),
      fact("Files hashed", `${plan.files} file(s), ${fmtBytes(plan.bytes)}`),
      fact("New blobs to publish", `${plan.new_blobs} (${fmtBytes(plan.new_bytes)})`),
      fact("Already in the origin", `${plan.shared_blobs} (${fmtBytes(plan.shared_bytes)})`),
      fact("Release sequence", releaseSequenceText(plan)),
      fact("What a player fetches", plan.player_fetch_note),
    ])
  );
}

async function pollPlan() {
  const answer = await call("op_status");
  if (!answer.ok) return;
  const state = answer.value;
  if (state.operation === "content" && state.plan && typeof state.plan === "object") {
    renderPlanFacts(state.plan);
  }
}

// -- startup ----------------------------------------------------------------

async function loadRemembered() {
  const answer = await call("publisher_settings");
  if (!answer.ok || !answer.value || !answer.value.remembered) return;
  const saved = answer.value.remembered.content;
  if (!saved || typeof saved !== "object") return;
  if (typeof saved.component === "string" && COMPONENTS.some(([value]) => value === saved.component)) {
    componentSelect.value = saved.component;
  }
  if (typeof saved.folder === "string") {
    folderInput.value = saved.folder;
  }
  if (typeof saved.game_id === "string") {
    gameIdSelect.value = saved.game_id;
  }
}

async function init() {
  await loadRemembered();
  renderLint();
  await loadVersionInfo();
}

init();
setInterval(pollPlan, POLL_MS);
