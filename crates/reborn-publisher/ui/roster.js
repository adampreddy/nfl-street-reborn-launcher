// Roster: publish the roster the website exported, or a .rebornroster
// file. One button: "Publish roster 2026.09.10". This is the job Adam runs
// most weeks.
//
// `job.js` drives the check, the plain-words card, the real publish and
// the read-back. Every number on the card comes straight off the plan
// value `roster.rs` returned.

import { call, el, partial, fmtBytes, markCurrent } from "./shell.js";
import { job } from "./job.js";

markCurrent();

const screen = document.getElementById("screen");

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

/// Today's date as `YYYY.MM.DD`, the label shape every roster version uses.
function todayLabel() {
  const d = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}.${pad(d.getMonth() + 1)}.${pad(d.getDate())}`;
}

// -- Source and version -----------------------------------------------------

const versionInput = el("input", { type: "text", value: todayLabel(), style: "width:140px" });
const kindSite = el("input", { type: "radio", name: "roster-source" });
const kindFile = el("input", { type: "radio", name: "roster-source" });
const fileInput = el("input", { type: "text", placeholder: String.raw`C:\path\to\roster.rebornroster`, style: "width:420px", disabled: true });

kindSite.checked = true;
kindSite.addEventListener("change", () => {
  fileInput.disabled = true;
});
kindFile.addEventListener("change", () => {
  fileInput.disabled = false;
  fileInput.focus();
});

function currentSource() {
  if (kindFile.checked) return { kind: "file", path: fileInput.value.trim() };
  return { kind: "site" };
}

const browseButton = el("button", {
  text: "Browse",
  onclick: async () => {
    const answer = await call("pick_file", { kind: "roster" });
    if (!answer.ok || !answer.value) return;
    fileInput.value = answer.value;
    kindFile.checked = true;
    fileInput.disabled = false;
  },
});

const sourceBlock = block(
  "Source and version",
  el(
    "div",
    { class: "row" },
    el("label", { class: "inline" }, kindSite, " The website's export"),
    el("label", { class: "inline" }, kindFile, " A .rebornroster file"),
    fileInput,
    browseButton
  ),
  el("p", { class: "hint", text: "The website's export needs the site token stored on Keys and settings." }),
  el("div", { class: "row", style: "margin-top:10px" }, el("label", { class: "inline" }, "Version ", versionInput)),
  el("p", { class: "hint", text: "Today's date, unless you want a different label." })
);

// -- Update notes ---------------------------------------------------------------

const NEWLINE = String.fromCharCode(10);
const notesInput = el("textarea", {
  rows: 5,
  placeholder: ["Roster Update:", "Added Joey Porter Jr. to the Cowboys", "Removed DeMarvion Overshown from the Cowboys"].join(NEWLINE),
  style: "width:100%;max-width:560px;font:inherit",
});
const notesBlock = block(
  "Update notes",
  notesInput,
  el("p", { class: "hint", text: "Players see this on the update card and once as What's new. One change per line; a line ending in a colon is a heading. Leave it empty for none. Notes never carry over to the next release." })
);
const currentNotes = () => notesInput.value.trim() || null;

// -- Remembering the last inputs ---------------------------------------------

async function loadRemembered() {
  const answer = await call("publisher_settings");
  if (!answer.ok) return;
  const remembered = (answer.value && answer.value.remembered && answer.value.remembered.roster) || {};
  if (typeof remembered.version === "string" && remembered.version.trim()) versionInput.value = remembered.version;
  if (remembered.source_kind === "file") {
    kindFile.checked = true;
    fileInput.disabled = false;
  } else {
    kindSite.checked = true;
  }
  if (typeof remembered.path === "string") fileInput.value = remembered.path;
  theJob.refresh();
}

function saveRemembered() {
  const src = currentSource();
  call("publisher_set_setting", {
    key: "remembered.roster",
    value: { version: versionInput.value.trim(), source_kind: src.kind, path: fileInput.value.trim() },
  });
}

// -- The job --------------------------------------------------------------------

const jobHost = el("div", {});

const theJob = job(jobHost, {
  operation: "roster",
  verb: () => (versionInput.value.trim() ? `Publish roster ${versionInput.value.trim()}` : ""),
  plan: () => {
    saveRemembered();
    return call("roster_plan", { source: currentSource(), version: versionInput.value.trim() });
  },
  run: (dryRun) => {
    saveRemembered();
    return call("roster_run", { source: currentSource(), version: versionInput.value.trim(), notes: currentNotes(), dry_run: dryRun });
  },
  verify: () => call("roster_verify"),
  describePlan: (plan) => {
    if (!plan || typeof plan !== "object" || !("players" in plan)) return [plan && plan.summary ? String(plan.summary) : "The check came back with nothing to describe."];
    const lines = [
      `${plan.players} players, ${plan.teams} teams, ${plan.portraits} portraits, exported from ${plan.base_label || "an unknown base"} (${fmtBytes(plan.file_bytes)}).`,
      `Players will be offered it as an update of ${fmtBytes(plan.file_bytes)}, published as roster ${plan.version || versionInput.value.trim()}.`,
    ];
    const notes = currentNotes();
    lines.push(notes
      ? "Update notes players will see: " + notes.split(NEWLINE).map((l) => l.trim()).filter(Boolean).join(" / ")
      : "No update notes: players see just the roster version.");
    if (plan.release_bootstrap_needed) {
      lines.push("Warning: this computer has not signed a release yet, so nothing else can be carried forward from here. Sign one from the command line first, with everything.");
    } else if (!plan.release_will_be_signed) {
      lines.push("Warning: no signing key is configured on Keys and settings. The roster would be published, but players would not get it until a release is signed.");
    }
    return lines;
  },
  describeDone: (state) => {
    const plan = state && state.plan && typeof state.plan === "object" ? state.plan : {};
    return `Done. Players now get roster ${plan.version || versionInput.value.trim()}.`;
  },
  confirmLabel: "Yes, publish",
  emptyLabel: "Type a version first",
});

versionInput.addEventListener("input", () => theJob.refresh());

notesInput.addEventListener("input", () => theJob.refresh());

screen.replaceChildren(sourceBlock, notesBlock, block("Publish", jobHost));

loadRemembered();
