// The Roster page: fetch the roster the site published, or a file, and
// publish it. This is the operation Adam runs most weeks.
//
// The operation itself runs through the shared panel (`panel.js`), which
// gives Plan, Run, Verify, cancel and a live log for free. `panel.js` only
// ever shows one flattened summary line for a plan, though, and this
// page's brief needs the actual counts, the sequence and pins before and
// after, and a plain sentence on screen before Run is clickable -- so,
// exactly like `history.js`'s rollback view, this file also polls
// `op_status` on its own, the same endpoint `panel.js` polls, purely to
// read `state.plan` and draw that richer breakdown beside the panel.
// Nothing here invents a fact or computes a publishing rule: every value
// drawn is either straight off `state.plan` (built in `roster.rs`) or a
// plain display format (`fmtBytes`) applied to a number that came from
// there. Run and Verify are not given the same bespoke treatment: neither
// step's rich result reaches `op_status` (only a `plan` step populates
// `state.plan`), so their whole report is the panel's own summary line
// and log, which `roster.rs` writes in full sentences for exactly that
// reason.
//
// Choosing a file is a plain text box beside a Browse button: `pick_file`
// (see `pick.rs`) opens a native "choose an existing file" dialog filtered
// for a `.rebornroster`, drops the chosen path into the box, and switches
// the radio to "A .rebornroster file" for whoever clicked Browse without
// first clicking the radio. `settings_browse` still exists for Settings
// fields; this page's file is not one of those.

import { call, el, fact, facts, partial, fmtBytes, markCurrent } from "./shell.js";
import { panel } from "./panel.js";

markCurrent();

const screen = document.getElementById("screen");
const POLL_MS = 400;

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

/// Today's date as `YYYY.MM.DD`, the label shape every roster and release
/// version already in this codebase uses (`2026.08.28`, `2026.09.06`).
function todayLabel() {
  const d = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}.${pad(d.getMonth() + 1)}.${pad(d.getDate())}`;
}

// -- Source and version ---------------------------------------------------

const versionInput = el("input", { type: "text", value: todayLabel(), style: "width:140px" });

const kindSite = el("input", { type: "radio", name: "roster-source" });
const kindFile = el("input", { type: "radio", name: "roster-source" });
const fileInput = el("input", {
  type: "text",
  placeholder: String.raw`C:\path\to\roster.rebornroster`,
  style: "width:420px",
  disabled: true,
});

kindSite.checked = true;
kindSite.addEventListener("change", () => {
  fileInput.disabled = true;
});
kindFile.addEventListener("change", () => {
  fileInput.disabled = false;
  fileInput.focus();
});

function currentSource() {
  if (kindFile.checked) {
    return { kind: "file", path: fileInput.value.trim() };
  }
  return { kind: "site" };
}

/// Opens the native picker for a `.rebornroster`, fills `fileInput` with
/// whatever was chosen, and switches the source radio to "A .rebornroster
/// file" so Browse works on its own without first clicking the radio. A
/// cancelled dialog leaves everything as it was.
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
    el("label", { class: "inline" }, kindSite, " The site's export"),
    el("label", { class: "inline" }, kindFile, " A .rebornroster file"),
    fileInput,
    browseButton
  ),
  el("p", { class: "hint", text: "The site's export needs the site token stored on the Keys page." }),
  el("div", { class: "row", style: "margin-top:10px" }, el("label", { class: "inline" }, "Version ", versionInput)),
  el("p", { class: "hint", text: "Defaults to today's date. Edit it for a different label." })
);

// -- Remembering the last inputs, the same way every other page does ------

async function loadRemembered() {
  const answer = await call("publisher_settings");
  if (!answer.ok) return;
  const remembered = (answer.value && answer.value.remembered && answer.value.remembered.roster) || {};
  if (typeof remembered.version === "string" && remembered.version.trim()) {
    versionInput.value = remembered.version;
  }
  if (remembered.source_kind === "file") {
    kindFile.checked = true;
    fileInput.disabled = false;
  } else {
    kindSite.checked = true;
  }
  if (typeof remembered.path === "string") {
    fileInput.value = remembered.path;
  }
}

function saveRemembered() {
  const src = currentSource();
  call("publisher_set_setting", {
    key: "remembered.roster",
    value: { version: versionInput.value.trim(), source_kind: src.kind, path: fileInput.value.trim() },
  });
}

// -- The operation panel ---------------------------------------------------

const panelHost = el("div", {});

panel(panelHost, {
  operation: "roster",
  describe:
    "Plan reads the roster's counts and shows the pins before and after. Run publishes the " +
    "component, signs a release carrying every other pin forward, and uploads. Verify reads the " +
    "hosted release back, then a seeded scratch install to confirm only the roster is pending.",
  plan: () => {
    saveRemembered();
    return call("roster_plan", { source: currentSource(), version: versionInput.value.trim() });
  },
  run: (dryRun) => {
    saveRemembered();
    return call("roster_run", { source: currentSource(), version: versionInput.value.trim(), dry_run: dryRun });
  },
  verify: () => call("roster_verify"),
});

// -- The plan's own counts, sequences and pins, read from `op_status` -----

function pinsTable(pairs) {
  return facts((pairs || []).map(([name, ver]) => fact(name, ver)));
}

function arrow(before, after) {
  const b = before === null || before === undefined ? "none yet" : before;
  const a = after === null || after === undefined ? "none yet" : after;
  return `${b} \u2192 ${a}`;
}

const planHost = block("Plan details", partial("Plan has not been run yet."));

let lastRenderedMarker = null;

function renderPlan(plan) {
  if (!plan || typeof plan !== "object" || !("players" in plan)) {
    return;
  }
  if (plan.refused || plan.error) {
    // A refused plan step never reaches `state.plan` (only a `Done` step
    // does, per `ops.rs`), so this only guards a plan value shaped
    // unexpectedly; the panel's own refusal styling is what a real
    // refusal shows.
    return;
  }
  const marker = JSON.stringify(plan);
  if (marker === lastRenderedMarker) return;
  lastRenderedMarker = marker;

  const children = [];

  children.push(
    facts([
      fact("Players", plan.players),
      fact("Teams", plan.teams),
      fact("Portraits", plan.portraits),
      fact("Base the roster was exported from", plan.base_label),
      fact("File size", fmtBytes(plan.file_bytes)),
    ])
  );

  children.push(
    el(
      "div",
      { class: "row", style: "align-items:flex-start;gap:24px;margin-top:12px" },
      el("div", {}, el("p", { class: "hint", text: "Pins before" }), pinsTable(plan.current_pins)),
      el("div", {}, el("p", { class: "hint", text: "Pins after" }), pinsTable(plan.proposed_pins))
    )
  );

  children.push(
    facts([
      fact("Roster component sequence", arrow(plan.component_sequence_before, plan.component_sequence_after)),
      fact(
        "Release sequence",
        plan.release_sequence_after != null
          ? arrow(plan.release_sequence_before, plan.release_sequence_after)
          : "no release signed in this origin yet"
      ),
    ])
  );

  children.push(
    el("p", {
      class: "said",
      text: `A player who already has the game installed will see this as an ask-first content update of ${fmtBytes(
        plan.file_bytes
      )}.`,
    })
  );

  if (plan.release_bootstrap_needed) {
    children.push(
      partial(
        "This origin has not signed a release yet, so the release step cannot carry any other pins forward from " +
          "here. Sign one from the command line first with every component, or pass a release version and " +
          "sequence explicitly."
      )
    );
  } else if (!plan.release_will_be_signed) {
    children.push(
      partial(
        "No publisher key is configured on the Keys page. The component would publish, but the release would " +
          "only be described, not signed."
      )
    );
  }

  planHost.replaceChildren(el("h2", { text: "Plan details" }), ...children);
}

async function pollPlan() {
  const answer = await call("op_status");
  if (!answer.ok) return;
  const state = answer.value;
  if (state.operation === "roster" && state.plan && typeof state.plan === "object") {
    renderPlan(state.plan);
  }
}

setInterval(pollPlan, POLL_MS);

screen.replaceChildren(sourceBlock, block("Publish", panelHost), planHost);

loadRemembered();
pollPlan();
