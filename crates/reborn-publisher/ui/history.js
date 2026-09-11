// Roll back: put an earlier version of one thing back, and the history of
// every publish this computer has done. One button: "Roll textures back
// to 1.1.0".
//
// The history (`history_list`) and the version list (`history_versions`)
// are plain reads. Rolling back runs through `job.js`: the check shows in
// plain words what players have now and what they would get; the real run
// signs and uploads; the read-back confirms the server serves it. Every
// number on the card comes straight off the plan value `history.rs`
// returned.

import { call, el, fact, facts, refusal, partial, fmtBytes, markCurrent, thingName, thingWord } from "./shell.js";
import { job } from "./job.js";

markCurrent();

// The wire names `reborn_core::inventory::ComponentId` recognizes.
const COMPONENTS = ["launcher", "runtime", "offline-content", "online-content", "textures", "mods", "controller-db", "roster", "game-patch"];

const JOURNAL_LIMIT = 200;

const screen = document.getElementById("screen");

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

const OPERATION_WORDS = {
  upload: "Upload to the server",
  status: "Upload to the server",
  content: "Textures, runtime or mods",
  launcher: "Launcher update",
  roster: "Roster",
  history: "Roll back",
  game_build: "Build an ISO",
  game_patch: "Game patch",
  set: "Full installer set",
};

// -- rollback ---------------------------------------------------------------

const componentSelect = el("select", { id: "rb-component" }, ...COMPONENTS.map((value) => el("option", { value, text: thingName(value) })));
// The roster is what gets rolled back in practice; said outright so the
// button's text never depends on the browser's default selection.
componentSelect.value = "roster";
const versionSelect = el("select", { id: "rb-version" });
const versionNote = el("p", { class: "hint" });
const jobHost = el("div");

function selection() {
  return { component: componentSelect.value, version: versionSelect.value };
}

async function loadVersions() {
  versionSelect.replaceChildren();
  versionNote.replaceChildren();
  const component = componentSelect.value;
  const answer = await call("history_versions", { component });
  if (!answer.ok) {
    versionNote.replaceChildren(refusal(answer.error));
    theJob.refresh();
    return;
  }
  const { current_version: current } = answer.value || {};
  const versions = Array.isArray(answer.value && answer.value.versions) ? answer.value.versions : [];
  const candidates = versions.filter((v) => !v.current);
  if (!versions.length) {
    versionNote.replaceChildren(partial(`This computer has never published ${thingWord(component)}, so there is nothing to go back to.`));
    theJob.refresh();
    return;
  }
  if (!current) {
    versionNote.replaceChildren(partial(`Players do not get ${thingWord(component)} from the server right now, so there is nothing to roll back from.`));
  } else {
    versionNote.replaceChildren(document.createTextNode(`Players have ${thingWord(component)} ${current} now.`));
  }
  for (const v of versions) {
    versionSelect.append(
      el("option", {
        value: v.version,
        disabled: v.current,
        selected: !v.current && v.version === (candidates[0] && candidates[0].version),
        text: v.current ? `${v.version} (what players have now)` : `${v.version} (${fmtBytes(v.bytes)})`,
      })
    );
  }
  // Said outright, not left to the browser's own idea of which option is
  // selected: the button's text is built from this value.
  versionSelect.value = candidates[0] ? candidates[0].version : "";
  theJob.refresh();
}

componentSelect.addEventListener("change", loadVersions);
versionSelect.addEventListener("change", () => theJob.refresh());

const theJob = job(jobHost, {
  operation: "history",
  verb: () => {
    const s = selection();
    if (!s.version) return "";
    return `Roll ${thingWord(s.component)} back to ${s.version}`;
  },
  plan: () => call("rollback_plan", selection()),
  run: (dryRun) => call("rollback_run", { ...selection(), dry_run: dryRun }),
  verify: () => call("rollback_verify"),
  describePlan: (plan) => {
    if (!plan || typeof plan !== "object" || !("component" in plan)) return [plan && plan.summary ? String(plan.summary) : "The check came back with nothing to describe."];
    const word = thingWord(plan.component);
    return [
      `Players have ${word} ${plan.current_version} now. They will get ${word} ${plan.target_version} instead.`,
      `Players on ${plan.current_version} will download ${fmtBytes(plan.fetch_bytes)} (${plan.changed_files} file(s)).`,
      "Everything else players get stays exactly as it is. Nothing is deleted from the server.",
    ];
  },
  describeDone: (state) => {
    const plan = state && state.plan && typeof state.plan === "object" ? state.plan : {};
    const s = selection();
    return `Done. Players now get ${thingWord(plan.component || s.component)} ${plan.target_version || s.version}.`;
  },
  confirmLabel: "Yes, roll back",
  emptyLabel: "Nothing to roll back to",
  onSettled: () => {
    loadJournal();
    loadVersions();
  },
});

// -- the history ----------------------------------------------------------------

const journalNote = el("p", { class: "hint" });
const journalBody = el("tbody");
const journalTable = el(
  "table",
  { class: "facts", style: "table-layout:auto" },
  el("thead", {}, el("tr", {}, el("th", { text: "When" }), el("th", { text: "What" }), el("th", { text: "Outcome" }), el("th", { text: "Summary" }))),
  journalBody
);
const journalWrap = el("div", { class: "capped-list" }, journalTable);

/// The journal stores UTC with an explicit offset on purpose; this is the
/// one place that converts it, in the browser, which knows what local is.
function localTime(iso) {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return `${iso} (could not be parsed)`;
  return d.toLocaleString([], { month: "numeric", day: "numeric", year: "2-digit", hour: "numeric", minute: "2-digit" });
}

function outcomeLabel(record) {
  const base = record.outcome || "unknown";
  return record.dry_run ? `${base} (check only)` : base;
}

function detailRow(record) {
  const rows = [
    fact("Time (UTC)", record.at),
    fact("Operation", record.operation),
    fact("Check only", record.dry_run ? "yes" : "no"),
    fact("Release number", record.sequence),
    fact("Hashes", Array.isArray(record.hashes) && record.hashes.length ? record.hashes.map(([name, hex]) => `${name} = ${hex}`).join("; ") : null),
    fact("Uploaded", Array.isArray(record.uploaded) && record.uploaded.length ? record.uploaded.join(", ") : null),
    fact("Verified", record.verified),
    fact("Inputs", JSON.stringify(record.inputs)),
  ];
  return el("tr", { hidden: true }, el("td", { colspan: "4" }, facts(rows)));
}

function journalRow(record) {
  const detail = detailRow(record);
  const main = el(
    "tr",
    { style: "cursor:pointer", title: "Click for the full record", onclick: () => { detail.hidden = !detail.hidden; } },
    el("td", { text: localTime(record.at) }),
    el("td", { text: OPERATION_WORDS[record.operation] || record.operation }),
    el("td", { text: outcomeLabel(record) }),
    el("td", { text: record.summary || "" })
  );
  return [main, detail];
}

async function loadJournal() {
  const answer = await call("history_list", { limit: JOURNAL_LIMIT });
  journalBody.replaceChildren();
  if (!answer.ok) {
    journalNote.replaceChildren(refusal(answer.error));
    return;
  }
  const { total, skipped } = answer.value || {};
  const records = Array.isArray(answer.value && answer.value.records) ? answer.value.records : [];
  const notes = [];
  if (total === 0 || (!total && records.length === 0)) notes.push("Nothing has been published from this computer yet.");
  else if (typeof total === "number" && records.length < total) notes.push(`Showing the newest ${records.length} of ${total}.`);
  if (skipped) notes.push(`${skipped} line(s) of the history could not be read and were skipped.`);
  journalNote.replaceChildren(notes.length ? partial(notes.join(" ")) : document.createTextNode(""));
  for (const record of records) journalRow(record).forEach((node) => journalBody.append(node));
}

screen.append(
  block(
    "Roll back",
    el("div", { class: "row" }, el("label", { class: "inline" }, "What", componentSelect), el("label", { class: "inline" }, "Back to", versionSelect)),
    versionNote,
    jobHost
  ),
  block("History", journalNote, journalWrap)
);

loadJournal();
loadVersions();
