// League files: publish the league build (the league disc + its texture
// folder) or the league seed (.s01). Two jobs, one button each:
// "Publish league build 2026.10.09" and "Publish league seed 2026.10.09".
//
// Both are members only. `league.rs` puts their files in the private
// league bucket first and only then uploads the manifest and the release
// to the public server. The check is the same command with dry_run: true;
// "Yes, publish" calls it again with dry_run: false. `job.js` drives both.
//
// No release notes here: notes become a "What's new" card for EVERY player,
// members or not, and a league publish is nothing non-members should see.

import { call, el, markCurrent } from "./shell.js";
import { job } from "./job.js";

markCurrent();

const screen = document.getElementById("screen");
const MEMBERS_ONLY = "Members only. Files go to the private league bucket; the public CDN never serves them.";

function block(title, ...children) {
  return el("section", { class: "block" }, el("h2", { text: title }), ...children);
}

/// Today's date as `YYYY.MM.DD`.
function todayLabel() {
  const d = new Date();
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}.${pad(d.getMonth() + 1)}.${pad(d.getDate())}`;
}

function pathRow(label, input, browse) {
  return el("div", { class: "row" }, el("label", { class: "inline" }, label, " ", input), browse);
}

function browseButton(input, command, args) {
  return el("button", {
    text: "Browse",
    onclick: async () => {
      const answer = await call(command, args);
      if (!answer.ok || !answer.value) return;
      input.value = answer.value;
      refreshAll();
    },
  });
}

function describe(plan) {
  if (plan && typeof plan === "object" && plan.summary) return [String(plan.summary)];
  return ["The check came back with nothing to describe."];
}

// -- League build ------------------------------------------------------------

const gameIso = el("input", { type: "text", placeholder: String.raw`C:\path\to\league.iso`, style: "width:420px" });
const gameTextures = el("input", { type: "text", placeholder: String.raw`C:\path\to\textures\<game id>`, style: "width:420px" });
const gameVersion = el("input", { type: "text", value: todayLabel(), style: "width:140px" });
const gameJobHost = el("div", {});

const gameJob = job(gameJobHost, {
  operation: "league_game",
  verb: () => (gameVersion.value.trim() ? `Publish league build ${gameVersion.value.trim()}` : ""),
  plan: () => {
    saveRemembered();
    return call("league_game_run", gameArgs(true));
  },
  run: () => {
    saveRemembered();
    return call("league_game_run", gameArgs(false));
  },
  describePlan: (plan) => describe(plan),
  describeDone: (state) => (state && state.summary ? String(state.summary) : `Done. League members now get league build ${gameVersion.value.trim()}.`),
  confirmLabel: "Yes, publish",
  emptyLabel: "Type a version first",
});

function gameArgs(dryRun) {
  return { iso: gameIso.value.trim(), textures: gameTextures.value.trim(), version: gameVersion.value.trim(), dry_run: dryRun };
}

const gameBlock = block(
  "League build",
  pathRow("Disc", gameIso, browseButton(gameIso, "pick_file", { kind: "disc image" })),
  pathRow("Textures", gameTextures, browseButton(gameTextures, "pick_folder", {})),
  el("p", { class: "hint", text: "An uncompressed league disc (its own game id, never a retail one) and its texture folder." }),
  el("div", { class: "row", style: "margin-top:10px" }, el("label", { class: "inline" }, "Version ", gameVersion)),
  el("p", { class: "hint", text: MEMBERS_ONLY }),
  gameJobHost
);

// -- League seed -------------------------------------------------------------

const seedFile = el("input", { type: "text", placeholder: String.raw`C:\path\to\league.s01`, style: "width:420px" });
const seedJson = el("input", { type: "text", placeholder: "Optional: seed.json", style: "width:420px" });
const seedVersion = el("input", { type: "text", value: todayLabel(), style: "width:140px" });
const seedJobHost = el("div", {});

const seedJob = job(seedJobHost, {
  operation: "league_content",
  verb: () => (seedVersion.value.trim() ? `Publish league seed ${seedVersion.value.trim()}` : ""),
  plan: () => {
    saveRemembered();
    return call("league_content_run", seedArgs(true));
  },
  run: () => {
    saveRemembered();
    return call("league_content_run", seedArgs(false));
  },
  describePlan: (plan) => describe(plan),
  describeDone: (state) => (state && state.summary ? String(state.summary) : `Done. League members now get league seed ${seedVersion.value.trim()}.`),
  confirmLabel: "Yes, publish",
  emptyLabel: "Type a version first",
});

function seedArgs(dryRun) {
  const json = seedJson.value.trim();
  return { seed: seedFile.value.trim(), seed_json: json || null, version: seedVersion.value.trim(), dry_run: dryRun };
}

const seedBlock = block(
  "League seed",
  pathRow("Seed", seedFile, browseButton(seedFile, "pick_file", { kind: "save state" })),
  pathRow("seed.json", seedJson, browseButton(seedJson, "pick_file", { kind: "json" })),
  el("p", { class: "hint", text: "The .s01 save state league games boot from, and optionally its seed.json capture record." }),
  el("div", { class: "row", style: "margin-top:10px" }, el("label", { class: "inline" }, "Version ", seedVersion)),
  el("p", { class: "hint", text: MEMBERS_ONLY }),
  seedJobHost
);

// -- Remembering the last inputs ---------------------------------------------

function refreshAll() {
  gameJob.refresh();
  seedJob.refresh();
}

async function loadRemembered() {
  const answer = await call("publisher_settings");
  if (!answer.ok) return;
  const r = (answer.value && answer.value.remembered && answer.value.remembered.league) || {};
  if (typeof r.iso === "string") gameIso.value = r.iso;
  if (typeof r.textures === "string") gameTextures.value = r.textures;
  if (typeof r.seed === "string") seedFile.value = r.seed;
  if (typeof r.seed_json === "string") seedJson.value = r.seed_json;
  refreshAll();
}

function saveRemembered() {
  call("publisher_set_setting", {
    key: "remembered.league",
    value: { iso: gameIso.value.trim(), textures: gameTextures.value.trim(), seed: seedFile.value.trim(), seed_json: seedJson.value.trim() },
  });
}

for (const input of [gameVersion, seedVersion]) input.addEventListener("input", refreshAll);

screen.replaceChildren(gameBlock, seedBlock);

loadRemembered();
