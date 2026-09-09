// The Launcher page: build the launcher-only installer and publish it with
// its update manifest.
//
// The version box is proposed from the bucket's current launcher version
// (read through `publisher_status`, the same call the Status page already
// makes) with its patch number bumped, and stays editable: nothing here
// forces a particular value. The panel (`panel.js`) drives Plan, Run and
// Verify exactly as every other page's does, so the busy state, the log
// pane and the Cancel/Clear buttons are all its own; a real build's output
// streams into that same log pane line by line as `launcher.rs` produces
// it, which is what keeps this page from looking frozen for the several
// minutes a real build takes.
//
// `panel.js` only ever shows one flattened summary line for a plan; this
// file also polls `op_status` on its own (same endpoint, same interval),
// purely to read `state.plan` and draw the fuller picture beside it: the
// warning when the version would offer nothing, what the bucket serves
// now, and what this build would use. Same pattern `history.js` uses for
// its own rollback plan. Nothing here invents a fact: every value drawn is
// either straight off `publisher_status` or straight off `op_status`.

import { call, el, fact, facts, partial, fmtBytes, markCurrent } from "./shell.js";
import { panel } from "./panel.js";

markCurrent();

const POLL_MS = 400;
const screen = document.getElementById("screen");

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

// -- the version box --------------------------------------------------

const versionInput = el("input", { type: "text", id: "launcher-version", placeholder: "e.g. 3.0.2" });
const versionNote = el("p", { class: "hint" });
const planHost = el("div");
const panelHost = el("div");

screen.append(
  block(
    "Build and publish",
    el(
      "p",
      { class: "hint" },
      "Plan reads what the bucket currently serves and warns if the version below would not " +
        "be newer than that -- the one mistake that offers players nothing. Run builds " +
        "installer\\windows\\build.ps1 -LauncherOnly for real even under Dry run, since a build " +
        "only writes into dist; Dry run skips hashing and the upload, the step that actually " +
        "changes what players are offered. Verify reads the manifest and the installer back over " +
        "HTTPS and confirms a launcher on the version this replaced would be offered this one."
    ),
    el("div", { class: "row" }, el("label", { class: "inline" }, "Version", versionInput)),
    versionNote,
    planHost,
    panelHost
  )
);

/// "3.0.1" -> "3.0.2". A version this app cannot parse as three dotted
/// numbers is left exactly as it was rather than guessed at; the field
/// stays editable either way.
function bumpPatch(version) {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec((version || "").trim());
  if (!m) return version || "";
  const [, major, minor, patch] = m;
  return `${major}.${minor}.${Number(patch) + 1}`;
}

async function proposeVersion() {
  const answer = await call("publisher_status");
  if (!answer.ok) {
    versionNote.replaceChildren(
      partial(`The bucket's current launcher version could not be read: ${answer.error}. Type a version by hand.`)
    );
    return;
  }
  const launcher = answer.value && answer.value.launcher;
  if (launcher && launcher.version) {
    versionInput.value = bumpPatch(launcher.version);
    versionNote.replaceChildren(
      document.createTextNode(`Proposed from the bucket's current ${launcher.version}. Edit it if you want a different one.`)
    );
  } else {
    versionNote.replaceChildren(
      partial("The bucket does not currently serve a launcher version this app could read. Type a version by hand.")
    );
  }
}

// -- the operation panel ------------------------------------------------

function version() {
  return versionInput.value.trim();
}

panel(panelHost, {
  operation: "launcher",
  describe:
    "Plan shows what the bucket serves now and warns when the version below would offer " +
    "nothing newer. Run builds the installer, hashes it itself, and uploads it before the " +
    "manifest that names it. Verify reads both back over HTTPS and confirms a launcher on the " +
    "previous version is offered this one.",
  plan: () => call("launcher_plan", { version: version() }),
  run: (dryRun) => call("launcher_run", { version: version(), dry_run: dryRun }),
  verify: () => call("launcher_verify", { version: version() }),
});

// -- the plan's own hosted-state facts, read from `op_status` -----------

let lastRenderedPlanAt = null;

function renderPlan(plan) {
  if (!plan || typeof plan !== "object" || !("proposed_version" in plan)) return;
  const marker = JSON.stringify(plan);
  if (marker === lastRenderedPlanAt) return;
  lastRenderedPlanAt = marker;

  const children = [];
  if (plan.warning) {
    children.push(el("p", { class: "refusal", role: "alert", text: plan.warning }));
  }

  const hosted = plan.hosted || {};
  if (hosted.reachable) {
    children.push(
      el("p", { class: "hint", text: "The bucket serves now:" }),
      facts([
        fact("Version", hosted.version),
        fact("Size", hosted.bytes != null ? fmtBytes(hosted.bytes) : null),
        fact("SHA-256", hosted.sha256),
        fact("File name", hosted.file_name),
      ])
    );
  } else {
    children.push(partial(`The bucket's launcher channel could not be read: ${hosted.why || "unknown reason"}.`));
  }

  const builder = plan.builder;
  children.push(
    el("p", { class: "hint", text: "This build would use:" }),
    facts([
      fact("Version", plan.proposed_version),
      fact("Output file", plan.setup_name),
      fact("Manifest URL", plan.manifest_url),
      fact("Repository commit", plan.commit ? (plan.dirty ? `${plan.commit} (with uncommitted changes)` : plan.commit) : null),
      fact("Reborn Builder", builder ? (builder.found ? builder.path : `not found: ${builder.path}`) : null),
    ])
  );

  planHost.replaceChildren(...children);
}

async function pollPlan() {
  const answer = await call("op_status");
  if (!answer.ok) return;
  const state = answer.value;
  if (state.operation === "launcher" && state.plan && typeof state.plan === "object") {
    renderPlan(state.plan);
  }
}

setInterval(pollPlan, POLL_MS);

proposeVersion();
pollPlan();
