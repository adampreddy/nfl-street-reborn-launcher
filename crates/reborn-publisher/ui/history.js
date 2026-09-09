// The History page: the journal as a table, and rollback to an earlier
// version of a component.
//
// The journal (`history_list`) and the version list (`history_versions`)
// are plain reads: this file calls them and draws what comes back, same as
// every other page. Rollback itself runs through the shared operation
// panel (`panel.js`), so it gets Plan, Run, Verify, cancel and a live log
// for free. `panel.js` only ever shows one flattened summary line for a
// plan, though, and the brief for this page needs the actual before/after
// pin table and the fetch-byte estimate on screen before Run is
// clickable -- so this file also polls `op_status` on its own, the same
// endpoint `panel.js` polls, purely to read `state.plan` and draw that
// richer table beside the panel. Nothing here invents a fact: every value
// drawn is either straight off a `history_*` call or straight off
// `op_status`, exactly like `panel.js` itself.

import { call, el, fact, facts, refusal, partial, fmtBytes, markCurrent } from "./shell.js";
import { panel } from "./panel.js";

markCurrent();

// The wire names `reborn_core::inventory::ComponentId` recognizes. Kept
// here rather than fetched, because no command lists them (every command
// that needs a component name takes one and validates it); this list is
// the same nine names `ComponentId::ALL` is settled at.
const COMPONENTS = [
  ["launcher", "Launcher"],
  ["runtime", "Runtime"],
  ["offline-content", "Offline content"],
  ["online-content", "Online content"],
  ["textures", "Textures"],
  ["mods", "Mods"],
  ["controller-db", "Controller DB"],
  ["roster", "Roster"],
  ["game-patch", "Game patch"],
];

const JOURNAL_LIMIT = 200;
const POLL_MS = 400;

const screen = document.getElementById("screen");

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

// -- the journal ----------------------------------------------------

const journalNote = el("p", { class: "hint" });
const journalBody = el("tbody");
const journalTable = el(
  "table",
  { class: "facts", style: "table-layout:auto" },
  el(
    "thead",
    {},
    el(
      "tr",
      {},
      el("th", { text: "When (local)" }),
      el("th", { text: "Operation" }),
      el("th", { text: "Outcome" }),
      el("th", { text: "Sequence" }),
      el("th", { text: "Summary" })
    )
  ),
  journalBody
);
const journalWrap = el("div", { class: "capped-list" }, journalTable);

screen.append(block("Journal", journalNote, journalWrap));

/// "2026-09-09T15:12:07+00:00" rendered in the viewer's own local time
/// zone, labelled. The journal stores UTC with an explicit offset on
/// purpose (see `journal::Record::at`'s doc); this is the one place that
/// converts it, since only the browser actually knows what "local" means
/// here.
function localTime(iso) {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return `${iso} (could not be parsed)`;
  return `${d.toLocaleString()} local`;
}

function outcomeLabel(record) {
  const base = record.outcome || "unknown";
  return record.dry_run ? `${base} (dry run)` : base;
}

/// One record's full detail, expanded: every field the journal actually
/// stores, timestamp shown both ways so a reader never has to guess which
/// one they are looking at.
function detailRow(record) {
  const rows = [
    fact("Schema", record.schema),
    fact("Time (UTC)", record.at),
    fact("Time (local)", localTime(record.at)),
    fact("Operation", record.operation),
    fact("Dry run", record.dry_run ? "yes" : "no"),
    fact("Sequence", record.sequence),
    fact(
      "Hashes",
      Array.isArray(record.hashes) && record.hashes.length
        ? record.hashes.map(([name, hex]) => `${name} = ${hex}`).join("; ")
        : null
    ),
    fact("Uploaded", Array.isArray(record.uploaded) && record.uploaded.length ? record.uploaded.join(", ") : null),
    fact("Verified", record.verified),
    fact("Outcome", record.outcome),
    fact("Summary", record.summary),
    fact("Inputs", JSON.stringify(record.inputs)),
  ];
  return el("tr", { hidden: true }, el("td", { colspan: "5" }, facts(rows)));
}

function journalRow(record) {
  const detail = detailRow(record);
  const main = el(
    "tr",
    { style: "cursor:pointer", title: "Click to expand the full record", onclick: () => { detail.hidden = !detail.hidden; } },
    el("td", { text: localTime(record.at) }),
    el("td", { text: record.operation }),
    el("td", { text: outcomeLabel(record) }),
    el("td", { text: record.sequence === null || record.sequence === undefined ? "" : String(record.sequence) }),
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
  const { total, skipped } = answer.value;
  // A field is absent, not an empty list, when the core answers with every
  // field null: `records` must never be assumed present here, the same way
  // `status.js` and `content.js` guard their own list answers.
  const records = Array.isArray(answer.value.records) ? answer.value.records : [];
  const notes = [];
  if (total === 0) {
    notes.push("No publishing operation has been recorded in this origin's journal yet.");
  } else if (records.length < total) {
    notes.push(`Showing the newest ${records.length} of ${total} record(s).`);
  }
  if (skipped) {
    notes.push(
      `${skipped} journal line(s) could not be read and were skipped. History here is incomplete.`
    );
  }
  journalNote.replaceChildren(notes.length ? partial(notes.join(" ")) : document.createTextNode(""));
  for (const record of records) {
    journalRow(record).forEach((node) => journalBody.append(node));
  }
}

// -- rollback ---------------------------------------------------------

const componentSelect = el(
  "select",
  { id: "rb-component" },
  ...COMPONENTS.map(([value, label]) => el("option", { value, text: label }))
);
const versionSelect = el("select", { id: "rb-version" });
const versionNote = el("p", { class: "hint" });
const factsHost = el("div");
const panelHost = el("div");

screen.append(
  block(
    "Roll back a component",
    el(
      "p",
      { class: "hint" },
      "Signs a new release at the next sequence pinning an earlier version of one " +
        "component; every other component stays exactly as it is served now. The " +
        "sequence goes up while the content goes back. Plan before Run: Run stays " +
        "disabled until a plan has come back, and rollback never happens without " +
        "its own click after that."
    ),
    el(
      "div",
      { class: "row" },
      el("label", { class: "inline" }, "Component", componentSelect),
      el("label", { class: "inline" }, "Roll back to", versionSelect)
    ),
    versionNote,
    factsHost,
    panelHost
  )
);

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
    return;
  }
  const { current_version: current } = answer.value;
  // Same guard `content.js` already applies to this same command's answer:
  // an absent `versions` is not an empty list.
  const versions = Array.isArray(answer.value.versions) ? answer.value.versions : [];
  const candidates = versions.filter((v) => !v.current);
  if (!versions.length) {
    versionNote.replaceChildren(partial(`The origin holds no manifest for ${component}. Nothing can be rolled back to.`));
    return;
  }
  if (!current) {
    versionNote.replaceChildren(
      partial(`${component} is not part of the currently signed release, so there is nothing to roll back from.`)
    );
  }
  for (const v of versions) {
    versionSelect.append(
      el("option", {
        value: v.version,
        disabled: v.current,
        selected: !v.current && v.version === (candidates[0] && candidates[0].version),
        text: v.current ? `${v.version} (currently live)` : `${v.version} (sequence ${v.sequence}, ${fmtBytes(v.bytes)})`,
      })
    );
  }
}

componentSelect.addEventListener("change", loadVersions);

panel(panelHost, {
  operation: "history",
  describe:
    "Plan shows the current pins against the proposed pins and the bytes a " +
    "player already on the live version would fetch. Run signs and uploads. " +
    "Verify reads the hosted release back and confirms the rolled-back pin.",
  plan: () => call("rollback_plan", selection()),
  run: (dryRun) => call("rollback_run", { ...selection(), dry_run: dryRun }),
  verify: () => call("rollback_verify"),
});

// -- the plan's own before/after, read from `op_status` -----------------
//
// `panel.js` shows `state.plan.summary` as one line. This renders the rest
// of the same object: the pins on each side, and the fetch estimate. It
// polls independently of the panel (same endpoint, same interval) because
// `panel.js` has no way to hand a page the structured value it drew from.

let lastRenderedPlanAt = null;
let lastPhase = null;

function pinsTable(pairs) {
  return facts((pairs || []).map(([name, version]) => fact(name, version)));
}

function renderPlan(plan) {
  if (!plan || typeof plan !== "object" || !("component" in plan)) {
    return;
  }
  const marker = JSON.stringify(plan);
  if (marker === lastRenderedPlanAt) return;
  lastRenderedPlanAt = marker;

  factsHost.replaceChildren(
    el(
      "div",
      { class: "row", style: "align-items:flex-start;gap:24px" },
      el("div", {}, el("p", { class: "hint", text: "Current pins" }), pinsTable(plan.current_pins)),
      el("div", {}, el("p", { class: "hint", text: "Proposed pins" }), pinsTable(plan.proposed_pins))
    ),
    facts([
      fact("Release sequence", `${plan.current_sequence} → ${plan.proposed_sequence}`),
      fact(
        "A player already on " + plan.current_version + " would fetch",
        `${fmtBytes(plan.fetch_bytes)} across ${plan.changed_files} file(s)`
      ),
      fact(plan.target_version + " in full", `${fmtBytes(plan.target_total_bytes)} across ${plan.target_total_files} file(s)`),
    ])
  );
}

async function pollPlanAndJournal() {
  const answer = await call("op_status");
  if (!answer.ok) return;
  const state = answer.value;
  if (state.operation === "history" && state.plan && typeof state.plan === "object") {
    renderPlan(state.plan);
  }
  // A run or a verify for this page just settled: the journal grew by one
  // record, so reload it. Caught on the busy-to-settled transition rather
  // than on every poll, so this does not refetch the journal four times a
  // second while idle.
  const busy = state.phase === "planning" || state.phase === "running" || state.phase === "verifying";
  if (!busy && lastPhase && state.operation === "history" && lastPhase !== state.phase) {
    loadJournal();
  }
  lastPhase = busy ? state.phase : null;
}

setInterval(pollPlanAndJournal, POLL_MS);

loadJournal();
loadVersions();
