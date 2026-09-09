// The Set page: build, upload and point the download link at the full
// first-run installer set -- the spanned installer plus its zip, the game
// import bundle, the Dolphin runtime, the texture pack and the offline
// WebView2 runtime. This is what a brand new player downloads.
//
// The build itself runs through the shared operation panel (`panel.js`),
// the same as every other page's long operation: Plan measures the four
// inputs and previews the exact zip file set, Run streams `build.ps1`'s own
// output live for the twenty minutes it takes, and Verify reads the
// published pointer back. `panel.js` only ever shows one flattened summary
// line for a plan, though, and this page's brief needs the richer picture
// (each input's size, what the bucket serves now, the exact files a zip
// would be packed from) -- so, exactly like `content.js` and `launcher.js`,
// this file also polls `op_status` on its own, the same endpoint `panel.js`
// polls, purely to read `state.plan` and draw that breakdown beside the
// panel. Nothing here invents a fact or computes a rule: every value drawn
// is either straight off `state.plan` (built in `set.rs`) or a plain
// display format (`fmtBytes`, `fmtCount`) applied to a number that came
// from there.
//
// The four folder fields are plain text boxes beside a Browse button
// (`pick_folder`, the same native picker `content.js` and `roster.js`
// already use), each remembered across runs under `remembered.set`, the
// same convention every other page's own inputs follow.

import { call, el, fact, facts, refusal, partial, fmtBytes, markCurrent } from "./shell.js";
import { panel } from "./panel.js";

markCurrent();

const screen = document.getElementById("screen");
const POLL_MS = 400;

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

/// A count the way a person reads a small integer: no formatting a plain
/// number needs, just a stable place to say "not known" the same way
/// `fmtBytes` does for a missing byte count.
function fmtCount(n) {
  return typeof n === "number" && Number.isFinite(n) ? String(n) : "unknown";
}

/// A Browse button beside a folder `input`: opens the native picker for a
/// folder and drops the chosen path straight into the field. A cancelled
/// dialog leaves the field alone.
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

const bundleInput = el("input", {
  type: "text",
  placeholder: String.raw`C:\Reborn Origin-stage\game-import-bundle-fields-12-19`,
  style: "min-width:420px",
});
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
}

for (const input of [bundleInput, runtimeInput, texturesInput, promptPackagesInput, versionInput]) {
  input.addEventListener("change", remember);
}

const inputsBlock = block(
  "Inputs",
  el(
    "p",
    { class: "hint" },
    "All remembered from the last run. The game-import bundle must not sit inside this repository's own " +
      "target\\installer-payload folder: build.ps1 deletes that folder before staging, and would delete " +
      "the bundle with it."
  ),
  el("div", { class: "row" }, el("label", { class: "inline" }, "Game-import bundle", bundleInput), browseFolderButton(bundleInput, remember)),
  el("div", { class: "row" }, el("label", { class: "inline" }, "Runtime tree", runtimeInput), browseFolderButton(runtimeInput, remember)),
  el("div", { class: "row" }, el("label", { class: "inline" }, "Texture tree", texturesInput), browseFolderButton(texturesInput, remember)),
  el("div", { class: "row" }, el("label", { class: "inline" }, "Prompt packages", promptPackagesInput), browseFolderButton(promptPackagesInput, remember)),
  el("div", { class: "row", style: "margin-top:10px" }, el("label", { class: "inline" }, "Version", versionInput))
);

// -- the panel ----------------------------------------------------------

const panelHost = el("div");

panel(panelHost, {
  operation: "set",
  describe:
    "Plan measures the four inputs, shows what launcher/download.json currently points at, and lists " +
    "exactly the files a zip for this version would be packed from. Run builds the full set for real " +
    "(about twenty minutes), confirms the zip it produced holds exactly that file set, uploads the zip, " +
    "then writes and uploads download.json last so the site is never pointed at a file still in flight. " +
    "Verify reads the pointer back and confirms it matches the local zip.",
  plan: () => call("set_plan", selection()),
  run: (dryRun) => call("set_run", { ...selection(), dry_run: dryRun }),
  verify: () => call("set_verify"),
});

// -- the plan's own richer facts, read from `op_status` ----------------

const planHost = block("Plan details", partial("Plan has not been run yet."));

let lastRenderedMarker = null;

function inputRow(label, entry) {
  if (!entry || typeof entry !== "object") {
    return fact(label, null);
  }
  return fact(label, `${entry.path || "not known"} -- ${fmtCount(entry.files)} file(s), ${fmtBytes(entry.bytes)}`);
}

function zipMembersList(members) {
  if (!Array.isArray(members) || members.length === 0) {
    return el("p", { class: "hint", text: "Nothing in dist matches yet; Run will produce it." });
  }
  return el(
    "table",
    { class: "facts" },
    ...members.map((m) => el("tr", {}, el("th", { text: m.name }), el("td", { text: fmtBytes(m.bytes) })))
  );
}

function renderPlan(plan) {
  if (!plan || typeof plan !== "object" || !("version" in plan)) {
    return;
  }
  const marker = JSON.stringify(plan);
  if (marker === lastRenderedMarker) return;
  lastRenderedMarker = marker;

  const children = [];

  if (plan.warning) {
    children.push(el("p", { class: "refusal", role: "alert", text: plan.warning }));
  }

  const hosted = plan.hosted || {};
  children.push(el("p", { class: "hint", text: "launcher/download.json serves now:" }));
  if (hosted.reachable === false) {
    children.push(partial(`Could not be read: ${hosted.why || "unknown reason"}.`));
  } else if (!hosted.version) {
    children.push(el("p", { class: "hint", text: "Nothing published yet; this would be the first." }));
  } else {
    children.push(
      facts([
        fact("Version", hosted.version),
        fact("Size", hosted.bytes != null ? fmtBytes(hosted.bytes) : null),
        fact("SHA-256", hosted.sha256),
        fact("Published", hosted.published),
        fact("URL", hosted.url),
      ])
    );
  }

  children.push(
    el("p", { class: "hint", text: "This build would use:" }),
    facts([fact("Version", plan.version), fact("Total across the four inputs", fmtBytes(plan.total_bytes))])
  );

  const inputs = plan.inputs || {};
  children.push(
    facts([
      inputRow("Bundle", inputs.bundle),
      inputRow("Runtime", inputs.runtime),
      inputRow("Textures", inputs.textures),
      inputRow("Prompt packages", inputs.prompt_packages),
    ])
  );

  if (plan.builder) {
    children.push(
      el("p", {
        class: plan.builder.found ? "hint" : "refusal",
        role: plan.builder.found ? null : "alert",
        text: plan.builder.found ? `Reborn Builder: ${plan.builder.path}` : `Reborn Builder not found at ${plan.builder.path}: build.ps1 will refuse to build until one is there.`,
      })
    );
  }

  const zip = plan.zip || {};
  children.push(
    el("p", { class: "hint", text: `Zip: ${zip.expected_name || "unknown"} -- exactly ${zip.expected_prefix || "?"}.exe plus its own numbered slices, never a wildcard.` }),
    zipMembersList(zip.existing_files)
  );

  planHost.replaceChildren(el("h2", { text: "Plan details" }), ...children);
}

async function pollPlan() {
  const answer = await call("op_status");
  if (!answer.ok) return;
  const state = answer.value;
  if (state.operation === "set" && state.plan && typeof state.plan === "object") {
    renderPlan(state.plan);
  }
}

setInterval(pollPlan, POLL_MS);

// -- startup ------------------------------------------------------------

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

screen.replaceChildren(inputsBlock, block("Build, upload, and point the link", panelHost), planHost);

async function init() {
  await loadRemembered();
  // The stated default: only the game-import bundle has one, per the
  // brief. The other three folders and the version stay blank until
  // remembered or typed, so an empty field is never mistaken for "this
  // machine's own answer".
  if (!bundleInput.value.trim()) {
    bundleInput.value = String.raw`C:\Reborn Origin-stage\game-import-bundle-fields-12-19`;
  }
}

init();
pollPlan();
