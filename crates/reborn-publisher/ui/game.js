// The Game page: Build ISO (through reborn-builder-cli.exe) and the game
// patch chain, both run through the shared operation panel from panel.js so
// both get Plan, Run and Verify with the same busy-lock, log and journal
// behavior every other publishing page has.
//
// Two things this file draws that the panel itself cannot, because the
// panel only ever shows a plan's own `summary` string (see panel.js's
// `planSummaryText`), never a structured value:
//
// - Build ISO's warnings, in a capped, scrolling list (`.capped-list`,
//   already defined in publisher.css for exactly this) with the Run button
//   relabeled "Build anyway" once Plan finds any. The Builder's own window
//   was rewritten after a build with 112 warnings pushed its buttons off
//   the screen; this list scrolls inside a fixed-height box so the buttons
//   below it never move, whatever the count.
// - Game patch's resolved base ("from"), so the operator sees which build
//   the chain thinks it is patching from before committing to Run.
//
// Both are read by polling `op_status` after a Plan or Run call returns,
// the same way panel.js itself does, because `game_build_plan` and
// `game_patch_plan` return only `{accepted: true}` immediately (the actual
// work runs on a worker thread; see `ops.rs`) and the structured result
// only ever lands in `op_status().plan`.
//
// `game_dump` and `game_cardsync` are different: they are synchronous,
// one-shot commands (see game.rs's module doc for why), so their answers
// are the real result immediately, with no polling needed.

import { el, call, partial, refusal, facts, fact, markCurrent } from "./shell.js";
import { panel } from "./panel.js";

markCurrent();

const screen = document.getElementById("screen");

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

function textInput(value, placeholder) {
  return el("input", { type: "text", value: value || "", placeholder: placeholder || "", style: "flex:1" });
}

function labeledRow(labelText, input) {
  return el("div", { class: "row" }, el("label", { text: labelText }), input);
}

/// Poll `op_status` until the operation it reports has settled (anything
/// but planning/running/verifying), so a caller that needs the structured
/// `plan` value a `*_plan` command just started can read it once it lands.
/// `panel.js` polls independently for its own display; this is a second,
/// harmless reader of the same cheap command.
async function waitForSettled(maxMs) {
  const deadline = Date.now() + (maxMs || 15 * 60 * 1000);
  while (Date.now() < deadline) {
    const r = await call("op_status");
    if (!r.ok) return r;
    const phase = (r.value && r.value.phase) || "idle";
    if (phase !== "planning" && phase !== "running" && phase !== "verifying") {
      return { ok: true, value: r.value };
    }
    await new Promise((resolve) => setTimeout(resolve, 350));
  }
  return { ok: false, error: "Timed out waiting for the operation to settle." };
}

async function rememberInputs(key, value) {
  await call("publisher_set_setting", { key: `remembered.${key}`, value });
}

// -- Build an ISO -----------------------------------------------------------

const baseInput = textInput("", "Base image (.iso)");
const rosterInput = textInput("", "Roster (.rebornroster)");
const outInput = textInput("", "Output ISO path");

const buildWarnings = el("div", { class: "capped-list" });
buildWarnings.hidden = true;

function renderBuildWarnings(list) {
  const warnings = Array.isArray(list) ? list : [];
  if (warnings.length === 0) {
    buildWarnings.hidden = true;
    buildWarnings.replaceChildren();
    return;
  }
  buildWarnings.hidden = false;
  buildWarnings.replaceChildren(
    el("p", { class: "hint", text: `${warnings.length} warning(s) from the site:` }),
    el("ul", {}, ...warnings.map((w) => el("li", { text: w })))
  );
}

let buildRunBtn = null;
let buildVerifyBtn = null;

function setBuildRunLabel(hasWarnings) {
  if (buildRunBtn) buildRunBtn.textContent = hasWarnings ? "Build anyway" : "Run";
}

const buildVerifyResult = el("div", {});

function renderBuildVerify(answer) {
  if (!answer.ok) {
    buildVerifyResult.replaceChildren(refusal(`Read-back failed: ${answer.error}`));
    return;
  }
  const v = answer.value || {};
  buildVerifyResult.replaceChildren(
    el("p", { class: "said", text: `Read back OK: ${v.summary || "(no summary)"}` }),
    facts([fact("ISO sha256 (computed here)", v.iso_sha256)])
  );
}

const buildPanelHost = el("div", {});
panel(buildPanelHost, {
  operation: "game_build",
  describe:
    "Runs reborn-builder-cli.exe against the base image and roster below. Plan always " +
    "previews into a scratch copy, never the output path; only Run, with dry run unchecked, " +
    "writes the real output.",
  plan: async () => {
    const values = { base: baseInput.value.trim(), roster: rosterInput.value.trim(), out: outInput.value.trim() };
    await rememberInputs("game_build", values);
    const initial = await call("game_build_plan", values);
    if (!initial.ok) {
      renderBuildWarnings([]);
      setBuildRunLabel(false);
      return initial;
    }
    const settled = await waitForSettled();
    const plan = settled.ok && settled.value ? settled.value.plan : null;
    const warnings = plan && Array.isArray(plan.warnings) ? plan.warnings : [];
    renderBuildWarnings(warnings);
    setBuildRunLabel(warnings.length > 0);
    return initial;
  },
  run: async (dryRun) => {
    const values = { base: baseInput.value.trim(), roster: rosterInput.value.trim(), out: outInput.value.trim() };
    await rememberInputs("game_build", values);
    return call("game_build_run", { ...values, dry_run: dryRun });
  },
  verify: async () => {
    const outPath = outInput.value.trim();
    if (!outPath) {
      const answer = { ok: false, error: "Set an output ISO path first, and build it, before verifying." };
      renderBuildVerify(answer);
      return answer;
    }
    if (buildVerifyBtn) buildVerifyBtn.disabled = true;
    const answer = await call("game_dump", { iso: outPath, out: `${outPath}.verify.rebornroster` });
    if (buildVerifyBtn) buildVerifyBtn.disabled = false;
    renderBuildVerify(answer);
    return answer;
  },
});
buildRunBtn =
  Array.from(buildPanelHost.querySelectorAll("button.write")).find(
    (b) => b.textContent === "Run" || b.textContent === "Build anyway"
  ) || null;
buildVerifyBtn = Array.from(buildPanelHost.querySelectorAll("button.write")).find((b) => b.textContent === "Verify") || null;

// -- Dump (one-shot) ----------------------------------------------------

const dumpIso = textInput("", "ISO to read");
const dumpOut = textInput("", "Write roster to (.rebornroster)");
const dumpResult = el("div", {});
const dumpBtn = el("button", {
  text: "Dump",
  onclick: async () => {
    dumpBtn.disabled = true;
    dumpResult.replaceChildren(el("p", { class: "hint", text: "Reading..." }));
    const answer = await call("game_dump", { iso: dumpIso.value.trim(), out: dumpOut.value.trim() });
    dumpBtn.disabled = false;
    if (!answer.ok) {
      dumpResult.replaceChildren(refusal(answer.error));
      return;
    }
    const v = answer.value || {};
    dumpResult.replaceChildren(
      el("p", { class: "said", text: v.summary || "Done." }),
      facts([fact("ISO sha256", v.iso_sha256), fact("Wrote", v.out)])
    );
  },
});

// -- Card sync (one-shot) -------------------------------------------------

const cardPath = textInput("", "Memory card (.raw)");
const cardRoster = textInput("", "Roster (.rebornroster)");
const cardDryRun = el("input", { type: "checkbox", checked: true });
const cardResult = el("div", {});
const cardBtn = el("button", {
  text: "Sync",
  onclick: async () => {
    cardBtn.disabled = true;
    cardResult.replaceChildren(el("p", { class: "hint", text: "Working..." }));
    const answer = await call("game_cardsync", {
      card: cardPath.value.trim(),
      roster: cardRoster.value.trim(),
      dry_run: cardDryRun.checked,
    });
    cardBtn.disabled = false;
    if (!answer.ok) {
      cardResult.replaceChildren(refusal(answer.error));
      return;
    }
    const v = answer.value || {};
    cardResult.replaceChildren(
      el("p", { class: v.dry_run ? "partial" : "said", text: (v.dry_run ? "(dry run) " : "") + (v.summary || "Done.") }),
      el("pre", { class: "panel-log", text: (v.lines || []).join("\n") })
    );
  },
});

function buildSection() {
  return block(
    "Build an ISO",
    el("p", {
      class: "hint",
      text:
        "Applies a roster to a base image and rebuilds a playable disc, through the same " +
        "reborn-builder-cli.exe the standalone Builder used. This is why that window is no " +
        "longer needed.",
    }),
    labeledRow("Base image (.iso)", baseInput),
    labeledRow("Roster (.rebornroster)", rosterInput),
    labeledRow("Output ISO path", outInput),
    buildWarnings,
    buildPanelHost,
    buildVerifyResult,
    el("h3", { text: "Dump" }),
    el("p", { class: "hint", text: "Read an ISO's current roster back out, for inspection." }),
    labeledRow("ISO", dumpIso),
    labeledRow("Output .rebornroster", dumpOut),
    dumpBtn,
    dumpResult,
    el("h3", { text: "Card sync" }),
    el("p", {
      class: "hint",
      text: "Bring a memory card's gear up to date with a roster, keeping in-game edits. Defaults to a dry run.",
    }),
    labeledRow("Memory card (.raw)", cardPath),
    labeledRow("Roster (.rebornroster)", cardRoster),
    el("label", { class: "inline" }, cardDryRun, " Dry run (say what would change; write nothing)"),
    cardBtn,
    cardResult
  );
}

// -- Game patch -----------------------------------------------------------

const toInput = textInput("", "New build (.iso)");
const versionInput = textInput(new Date().toISOString().slice(0, 10), "Version label");
const patchFromFact = el("div", {});

const patchPanelHost = el("div", {});
panel(patchPanelHost, {
  operation: "game_patch",
  describe:
    "Reads the chain's current tip, hashes the new build below itself, and refuses a base " +
    "that is not the tip, showing both hashes, before encoding, decode-verifying, publishing " +
    "and re-signing.",
  plan: async () => {
    const to = toInput.value.trim();
    await rememberInputs("game_patch_inputs", { to, version: versionInput.value.trim() });
    const initial = await call("game_patch_plan", { to });
    if (!initial.ok) return initial;
    const settled = await waitForSettled();
    const plan = settled.ok && settled.value ? settled.value.plan : null;
    if (plan && plan.from) {
      patchFromFact.replaceChildren(facts([fact("Chain's current base (from)", plan.from), fact("New build (to)", plan.to)]));
    }
    return initial;
  },
  run: async (dryRun) => {
    const values = { to: toInput.value.trim(), version: versionInput.value.trim() };
    await rememberInputs("game_patch_inputs", values);
    return call("game_patch_run", { ...values, dry_run: dryRun });
  },
  verify: () => call("game_patch_verify"),
});

function patchSection() {
  return block(
    "Game patch",
    el("p", {
      class: "hint",
      text:
        "Publishes a new game build as a delta from the build players already have. A patch " +
        "made from the wrong base is useless to everyone, so this always reads the chain's own " +
        "recorded tip rather than assuming it.",
    }),
    patchFromFact,
    labeledRow("New build (.iso)", toInput),
    labeledRow("Version label", versionInput),
    patchPanelHost
  );
}

// -- Remembered inputs, read once at load ----------------------------------

async function loadDefaults() {
  const answer = await call("publisher_settings");
  if (!answer.ok) {
    patchFromFact.replaceChildren(partial(`Settings could not be read: ${answer.error}`));
    return;
  }
  const settings = answer.value || {};
  const remembered = settings.remembered || {};
  const env = remembered.environment || {};
  const gameBuild = remembered.game_build || {};
  const gamePatchInputs = remembered.game_patch_inputs || {};
  const gamePatch = remembered.game_patch || {};

  baseInput.value = gameBuild.base || env.base_image || "";
  rosterInput.value = gameBuild.roster || "";
  outInput.value = gameBuild.out || "";

  if (gamePatchInputs.to) toInput.value = gamePatchInputs.to;
  if (gamePatchInputs.version) versionInput.value = gamePatchInputs.version;

  const currentBuild = gamePatch.current_build || env.base_image || null;
  if (currentBuild) {
    patchFromFact.replaceChildren(
      facts([fact("Chain's current base, from settings (not yet confirmed by Plan)", currentBuild)])
    );
  } else {
    patchFromFact.replaceChildren(
      partial(
        "No base image is configured yet. Set the Base image (FIELDS 12-19) on the Status " +
          "page's This Machine block, then Plan below."
      )
    );
  }
}

screen.replaceChildren(buildSection(), patchSection());
loadDefaults();
