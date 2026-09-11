// Build an ISO: build a playable disc through reborn-builder-cli.exe, read
// one back, sync a memory card, and publish a game patch. Two jobs
// ("Build the ISO", "Publish game patch <label>") through `job.js`, plus
// two one-shot tools (Dump, Card sync) whose commands answer at once.
//
// The build's warnings (the site can flag a hundred of them) are drawn in
// a capped, scrolling list so the buttons below never move; the confirm
// button reads "Build anyway" when there are any. That list is filled from
// the plan value the backend returned, handed over by `job.js`'s
// `onSettled`.

import { el, call, partial, refusal, facts, fact, markCurrent } from "./shell.js";
import { job } from "./job.js";

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

function browseButton(input, kind, save) {
  return el("button", {
    text: "Browse",
    onclick: async () => {
      const answer = await call(save ? "pick_save_file" : "pick_file", { kind });
      if (answer.ok && answer.value) {
        input.value = answer.value;
        buildJob.refresh();
        patchJob.refresh();
      }
    },
  });
}

function labeledPathRow(labelText, input, kind, save) {
  return el("div", { class: "row" }, el("label", { text: labelText }), input, browseButton(input, kind, save));
}

async function rememberInputs(key, value) {
  await call("publisher_set_setting", { key: `remembered.${key}`, value });
}

function shortHash(h) {
  return typeof h === "string" && h.length > 12 ? `${h.slice(0, 12)}...` : String(h || "unknown");
}

// -- Build an ISO -----------------------------------------------------------

const baseInput = textInput("", "Base image (.iso)");
const rosterInput = textInput("", "Roster (.rebornroster)");
const outInput = textInput("", "Output ISO path");

const buildWarnings = el("div", { class: "capped-list" });
buildWarnings.hidden = true;
let warningCount = 0;

function renderBuildWarnings(list) {
  const warnings = Array.isArray(list) ? list : [];
  warningCount = warnings.length;
  if (warnings.length === 0) {
    buildWarnings.hidden = true;
    buildWarnings.replaceChildren();
    return;
  }
  buildWarnings.hidden = false;
  buildWarnings.replaceChildren(el("p", { class: "hint", text: `${warnings.length} problem(s) the site flagged:` }), el("ul", {}, ...warnings.map((w) => el("li", { text: w }))));
}

function buildValues() {
  return { base: baseInput.value.trim(), roster: rosterInput.value.trim(), out: outInput.value.trim() };
}

const buildJobHost = el("div", {});
const buildJob = job(buildJobHost, {
  operation: "game_build",
  verb: () => {
    const v = buildValues();
    return v.base && v.roster && v.out ? "Build the ISO" : "";
  },
  plan: async () => {
    renderBuildWarnings([]);
    const values = buildValues();
    await rememberInputs("game_build", values);
    return call("game_build_plan", values);
  },
  run: async (dryRun) => {
    const values = buildValues();
    await rememberInputs("game_build", values);
    return call("game_build_run", { ...values, dry_run: dryRun });
  },
  describePlan: (plan) => {
    if (!plan || typeof plan !== "object") return ["The check came back with nothing to describe."];
    const lines = [];
    if (plan.players != null) lines.push(`${plan.players} players, ${plan.teams} teams, ${plan.portraits} portraits, from ${plan.base_label || "an unknown base"}.`);
    else if (plan.summary) lines.push(String(plan.summary));
    const warnings = Array.isArray(plan.warnings) ? plan.warnings : [];
    lines.push(warnings.length ? `Warning: ${warnings.length} problem(s) the site flagged. They are listed above.` : "The site flagged no problems.");
    lines.push(`The disc is written to ${outInput.value.trim() || "the output path"}.`);
    return lines;
  },
  describeDone: () => `Done. The disc is at ${outInput.value.trim()}. Read it back below if you want to be sure.`,
  confirmLabel: () => (warningCount > 0 ? "Build anyway" : "Yes, build"),
  onSettled: (state) => {
    if (state && state.phase === "planned" && state.plan && typeof state.plan === "object") renderBuildWarnings(state.plan.warnings);
  },
});

// -- Dump (one-shot) ----------------------------------------------------

const dumpIso = textInput("", "ISO to read");
const dumpOut = textInput("", "Write roster to (.rebornroster)");
const dumpResult = el("div", {});
const dumpBtn = el("button", {
  text: "Read the roster back",
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
    dumpResult.replaceChildren(el("p", { class: "said", text: v.summary || "Done." }), facts([fact("ISO SHA-256", v.iso_sha256), fact("Wrote", v.out)]));
  },
});

// -- Card sync (one-shot) -------------------------------------------------

const cardPath = textInput("", "Memory card (.raw)");
const cardRoster = textInput("", "Roster (.rebornroster)");
const cardPreview = el("input", { type: "checkbox", checked: true });
const cardResult = el("div", {});
const cardBtn = el("button", {
  text: "Sync",
  onclick: async () => {
    cardBtn.disabled = true;
    cardResult.replaceChildren(el("p", { class: "hint", text: "Working..." }));
    const answer = await call("game_cardsync", { card: cardPath.value.trim(), roster: cardRoster.value.trim(), dry_run: cardPreview.checked });
    cardBtn.disabled = false;
    if (!answer.ok) {
      cardResult.replaceChildren(refusal(answer.error));
      return;
    }
    const v = answer.value || {};
    cardResult.replaceChildren(
      el("p", { class: v.dry_run ? "partial" : "said", text: (v.dry_run ? "(preview) " : "") + (v.summary || "Done.") }),
      el("details", { class: "job-details" }, el("summary", { text: "Details" }), el("pre", { class: "panel-log", text: (v.lines || []).join("\n") }))
    );
  },
});

function buildSection() {
  return block(
    "Build an ISO",
    labeledPathRow("Base image (.iso)", baseInput, "disc image"),
    labeledPathRow("Roster (.rebornroster)", rosterInput, "roster"),
    labeledPathRow("Output ISO path", outInput, "disc image", true),
    buildWarnings,
    buildJobHost,
    el("h3", { text: "Read a disc's roster back" }),
    labeledPathRow("ISO", dumpIso, "disc image"),
    labeledPathRow("Write roster to", dumpOut, "roster", true),
    dumpBtn,
    dumpResult,
    el("h3", { text: "Card sync" }),
    el("p", { class: "hint", text: "Bring a memory card's gear up to date with a roster, keeping in-game edits." }),
    labeledPathRow("Memory card (.raw)", cardPath, "memory card"),
    labeledPathRow("Roster (.rebornroster)", cardRoster, "roster"),
    el("label", { class: "inline" }, cardPreview, " Preview only, change nothing"),
    cardBtn,
    cardResult
  );
}

// -- Game patch -----------------------------------------------------------

const toInput = textInput("", "New build (.iso)");
const versionInput = textInput(new Date().toISOString().slice(0, 10), "Version label");
const patchFromFact = el("div", {});

const patchJobHost = el("div", {});
const patchJob = job(patchJobHost, {
  operation: "game_patch",
  verb: () => (toInput.value.trim() && versionInput.value.trim() ? `Publish game patch ${versionInput.value.trim()}` : ""),
  plan: async () => {
    const to = toInput.value.trim();
    await rememberInputs("game_patch_inputs", { to, version: versionInput.value.trim() });
    return call("game_patch_plan", { to });
  },
  run: async (dryRun) => {
    const values = { to: toInput.value.trim(), version: versionInput.value.trim() };
    await rememberInputs("game_patch_inputs", values);
    return call("game_patch_run", { ...values, dry_run: dryRun });
  },
  verify: () => call("game_patch_verify"),
  describePlan: (plan) => {
    if (!plan || typeof plan !== "object") return ["The check came back with nothing to describe."];
    const lines = [];
    if (plan.from && plan.to) lines.push(`Patches the disc players have now (${shortHash(plan.from)}) into the new one (${shortHash(plan.to)}).`);
    else if (plan.summary) lines.push(String(plan.summary));
    lines.push(`Published as game patch ${versionInput.value.trim()}. The launcher patches each player's disc in place; no whole image is downloaded.`);
    return lines;
  },
  describeDone: () => `Done. Players now get game patch ${versionInput.value.trim()}.`,
  confirmLabel: "Yes, publish",
  onSettled: (state) => {
    const plan = state && state.plan;
    if (state && state.phase === "planned" && plan && plan.from) {
      patchFromFact.replaceChildren(facts([fact("Disc players have now", plan.from), fact("New disc", plan.to)]));
    }
  },
});

for (const input of [baseInput, rosterInput, outInput]) input.addEventListener("input", () => buildJob.refresh());
for (const input of [toInput, versionInput]) input.addEventListener("input", () => patchJob.refresh());

function patchSection() {
  return block("Game patch", patchFromFact, labeledPathRow("New build (.iso)", toInput, "disc image"), labeledRow("Version label", versionInput), patchJobHost);
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
    patchFromFact.replaceChildren(facts([fact("Disc players have now, from settings (the check confirms it)", currentBuild)]));
  } else {
    patchFromFact.replaceChildren(partial("No base image is configured yet. Set it on Keys and settings, then press the button below."));
  }
  buildJob.refresh();
  patchJob.refresh();
}

screen.replaceChildren(buildSection(), patchSection());
loadDefaults();
