// Launcher update: build the launcher installer and offer it to every
// player. One button: "Build and publish launcher 3.0.2".
//
// The version box is proposed from what the server serves now (read
// through `publisher_status`) with its last number bumped, and stays
// editable. `job.js` drives the check, the card, the real build and
// upload, and the read-back; a real build's output streams into the
// card's Details line by line as `launcher.rs` produces it.

import { call, el, partial, fmtBytes, markCurrent } from "./shell.js";
import { job } from "./job.js";

markCurrent();

const screen = document.getElementById("screen");

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

const versionInput = el("input", { type: "text", id: "launcher-version", placeholder: "e.g. 3.0.2", style: "width:140px" });
const versionNote = el("p", { class: "hint" });
const jobHost = el("div");

/// "3.0.1" -> "3.0.2". Anything else is left as it was.
function bumpPatch(version) {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec((version || "").trim());
  if (!m) return version || "";
  const [, major, minor, patch] = m;
  return `${major}.${minor}.${Number(patch) + 1}`;
}

let serverVersion = null;

async function proposeVersion() {
  const answer = await call("publisher_status");
  if (!answer.ok) {
    versionNote.replaceChildren(partial(`The launcher version on the server could not be read: ${answer.error}. Type a version by hand.`));
    theJob.refresh();
    return;
  }
  const launcher = answer.value && answer.value.launcher;
  if (launcher && launcher.version) {
    serverVersion = launcher.version;
    if (!versionInput.value.trim()) versionInput.value = bumpPatch(launcher.version);
    versionNote.replaceChildren(document.createTextNode(`Players have launcher ${launcher.version} now. Next: ${bumpPatch(launcher.version)}.`));
  } else {
    versionNote.replaceChildren(partial("The server does not serve a launcher version this app could read. Type a version by hand."));
  }
  theJob.refresh();
}

function version() {
  return versionInput.value.trim();
}

const theJob = job(jobHost, {
  operation: "launcher",
  verb: () => (version() ? `Build and publish launcher ${version()}` : ""),
  plan: () => call("launcher_plan", { version: version() }),
  run: (dryRun) => call("launcher_run", { version: version(), dry_run: dryRun }),
  verify: () => call("launcher_verify", { version: version() }),
  describePlan: (plan) => {
    if (!plan || typeof plan !== "object" || !("proposed_version" in plan)) return [plan && plan.summary ? String(plan.summary) : "The check came back with nothing to describe."];
    const lines = [];
    if (plan.warning) lines.push(`Warning: ${plan.warning}`);
    const hosted = plan.hosted || {};
    if (hosted.reachable) {
      lines.push(`Players have launcher ${hosted.version} now (${fmtBytes(hosted.bytes)}).`);
    } else {
      lines.push(`Warning: the launcher on the server could not be read: ${hosted.why || "unknown reason"}.`);
    }
    const commit = plan.commit ? ` from commit ${String(plan.commit).slice(0, 10)}${plan.dirty ? " with uncommitted changes" : ""}` : "";
    lines.push(`This builds ${plan.setup_name || "the installer"}${commit}, uploads it, then offers launcher ${plan.proposed_version} to everyone on ${hosted.version || "the current version"}.`);
    const builder = plan.builder;
    if (builder && !builder.found) lines.push(`Warning: the Reborn Builder is not at ${builder.path}; the build will refuse until it is.`);
    lines.push("The build itself takes several minutes.");
    return lines;
  },
  describeDone: (state) => {
    const plan = state && state.plan && typeof state.plan === "object" ? state.plan : {};
    return `Done. Players on ${serverVersion || "the previous version"} are now offered launcher ${plan.proposed_version || version()}.`;
  },
  confirmLabel: "Yes, build and publish",
  emptyLabel: "Type a version first",
  onSettled: () => proposeVersion(),
});

versionInput.addEventListener("input", () => theJob.refresh());

screen.append(block("Build and publish", el("div", { class: "row" }, el("label", { class: "inline" }, "Version", versionInput)), versionNote, jobHost));

proposeVersion();
