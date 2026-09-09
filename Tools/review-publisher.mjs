// A real browser against the real Publisher pages: Playwright, the same way
// review-onboarding.mjs and review-players.mjs drive the launcher's own
// pages. Two things check-publisher-pages.mjs's fake DOM cannot see at all:
//
//   - The operation panel's contract, the one every page but Keys depends on
//     (panel.js): Run stays disabled until Plan has returned; Plan (always a
//     dry run; see ops.rs's own doc, "a plan step is never anything else")
//     never itself calls the run command; a refusal shows its reason and
//     leaves Run available rather than stuck; a failure partway through Run
//     leaves the log showing what already happened instead of going blank.
//
//   - Actual layout. The standalone Builder's own window had to be rewritten
//     once a build with 112 warnings pushed its buttons off the screen and
//     the operator could not click Build at all; a fake DOM has no notion of
//     "off screen". This drives the Game page to that same 112-warning
//     state and measures the Build button's bounding box against the real
//     viewport, rather than trusting that `.capped-list`'s CSS does what its
//     comment says.
//
// One screenshot per page lands in target/publisher-shots/; the task this
// file was written for says to look at every one, not just assert against
// them, so this file's own report calls that out rather than treating the
// screenshots as a formality.
//
//     node Tools/review-publisher.mjs

import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";
import assert from "node:assert/strict";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const ui = path.join(root, "crates/reborn-publisher/ui");
const out = path.resolve(process.argv[2] || path.join(root, "target/publisher-shots"));
mkdirSync(out, { recursive: true });

// The same FULL fixture check-publisher-pages.mjs builds for its fake DOM,
// pulled out the same way review-onboarding.mjs pulls the shell app's own
// FULL out of check-shell-pages.mjs: it is a plain object literal, so a tiny
// slice of the source plus `vm.runInNewContext` gets it without duplicating
// nine command shapes by hand a second time in this file.
const checkerSource = readFileSync(path.join(root, "Tools/check-publisher-pages.mjs"), "utf8");
const FULL = vm.runInNewContext(
  checkerSource.slice(checkerSource.indexOf("const FULL = {"), checkerSource.indexOf("// Every field null")) + "\nFULL"
);

const types = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css" };
const server = createServer((req, res) => {
  try {
    const pathname = decodeURIComponent(new URL(req.url, "http://localhost").pathname);
    const file = path.resolve(ui, "." + (pathname === "/" ? "/index.html" : pathname));
    if (!file.startsWith(ui + path.sep)) throw new Error("outside");
    res.writeHead(200, { "Content-Type": types[path.extname(file)] || "application/octet-stream" });
    res.end(readFileSync(file));
  } catch {
    res.writeHead(404);
    res.end("Not found");
  }
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const url = `http://127.0.0.1:${server.address().port}`;

const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || "msedge", headless: true });
const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });

// One fake core, shared by every page this file opens. Plain reads (the
// nine commands FULL answers) come straight out of the fixture; `op_status`
// is always answered from `window.__op`, a tiny state machine this script
// drives directly, since the operation panel's contract is exactly about
// phase transitions a static fixture cannot express. `roster_plan`,
// `roster_run` and `game_build_plan` are the three commands this file's
// scenarios actually drive; everything else a page calls on load falls back
// to the static fixture.
await context.addInitScript((data) => {
  window.__calls = [];
  window.__op = { phase: "idle", operation: "", lines: [], summary: "", plan: null, dry_run: false };
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args = {}) => {
        window.__calls.push({ cmd, args });
        if (window.__delayCommand === cmd) {
          await new Promise((resolve) => {
            window.__releaseDelayed = resolve;
          });
        }
        if (window.__reject === cmd) throw new Error(window.__rejectMessage || "Test failure: " + cmd);
        if (cmd === "op_status") return JSON.parse(JSON.stringify(window.__op));
        if (cmd === "publisher_set_setting") return {};
        if (cmd === "roster_plan") {
          if (window.__rosterPlanMode === "refuse") {
            window.__op = {
              phase: "refused", operation: "roster", lines: ["Roster        C:/path/roster.rebornroster"],
              summary: "A User folder was found under this tree. Refusing to publish a scratch install by mistake.",
              plan: null, dry_run: true,
            };
          } else {
            // `roster.rs`'s own `roster_plan` puts the same one-line
            // `summary` both at the top of the Done value AND inside it
            // (ops.rs's `finish` lifts `value.summary` into `state.summary`
            // verbatim); a fixture that leaves `plan.summary` out, unlike
            // the real command, makes panel.js's `planSummaryText` fall
            // back to dumping the whole plan object as JSON instead of
            // showing that sentence, which very nearly got reported here as
            // a page defect before this fixture was corrected to match.
            const summary = "Roster 2026.09.09: 569 players, 32 teams, 2,048 byte(s). Roster sequence none yet -> 1.";
            window.__op = {
              phase: "planned", operation: "roster",
              lines: ["Roster        C:/path/roster.rebornroster", "Read          569 players, 32 teams, 40 portrait(s)"],
              summary,
              plan: {
                summary,
                players: 569, teams: 32, portraits: 40, base_label: "2026.08.28", file_bytes: 2048,
                current_pins: [], proposed_pins: [["roster", "2026.09.09"]],
                component_sequence_before: null, component_sequence_after: 1,
                release_sequence_before: null, release_sequence_after: null,
                release_bootstrap_needed: true, release_will_be_signed: false,
              },
              dry_run: true,
            };
          }
          return { accepted: true };
        }
        if (cmd === "roster_run") {
          window.__op = {
            phase: "running", operation: "roster", lines: ["Publishing roster 2026.09.09..."],
            summary: "", plan: window.__op.plan, dry_run: !!args.dry_run,
          };
          await new Promise((r) => setTimeout(r, 30));
          if (window.__rosterRunMode === "fail") {
            window.__op = {
              ...window.__op, phase: "failed",
              lines: [...window.__op.lines, "Hashing roster-2026.09.09.json...", "ERROR: network unreachable"],
              summary: "network unreachable",
            };
          } else {
            window.__op = {
              ...window.__op, phase: "done",
              lines: [...window.__op.lines, "Uploaded 2 object(s)."],
              summary: "Published roster 2026.09.09.",
            };
          }
          return { accepted: true };
        }
        if (cmd === "roster_verify") {
          window.__op = { ...window.__op, phase: "done", summary: "Confirmed: the hosted release pins roster 2026.09.09." };
          return { accepted: true };
        }
        if (cmd === "game_build_plan") {
          // The exact scenario this task's brief names: a build with 112
          // warnings from the site, once the reason the standalone Builder's
          // window had to be rewritten because its own buttons went off
          // screen. `summary` is a short, fixed sentence here, matching
          // game.rs's own `game_build_plan_body`: the real command always
          // sends one line, never the warning list itself, in `plan.summary`
          // (see panel.js's `planSummaryText`, which falls back to
          // `JSON.stringify(plan)` only when `plan.summary` is absent).
          const warnings = Array.from(
            { length: 112 },
            (_, i) => `Player ${i + 1}: number ${(i % 99) + 1} is already worn by another player on this team.`
          );
          const summary = `${warnings.length} problem(s) the site flagged. Nothing was built. Review them, then Build anyway.`;
          window.__op = {
            phase: "planned", operation: "game_build",
            lines: ["Checked base image and roster.", summary],
            summary,
            plan: { summary, warnings, players: null, teams: null, portraits: null, base_label: null, preview_sha256: null },
            dry_run: true,
          };
          return { accepted: true };
        }
        if (cmd in data) return data[cmd];
        throw new Error("no fixture for " + cmd);
      },
    },
  };
}, FULL);

const page = await context.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));

const shot = async (name) => {
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(200);
  await page.screenshot({ path: path.join(out, name + ".png") });
};

// ---------------------------------------------------------------------------
// The operation panel's contract, driven on the Roster page: one panel, no
// ambiguity about which Plan/Run/Verify button is meant.
// ---------------------------------------------------------------------------

// 1. Run is disabled until Plan returns; Plan never itself calls Run.
await page.goto(url + "/roster.html");
const runBtn = page.getByRole("button", { name: "Run", exact: true });
assert.equal(await runBtn.isDisabled(), true, "Run must start disabled, before Plan has ever been clicked");
await page.evaluate(() => (window.__delayCommand = "roster_plan"));
await page.getByRole("button", { name: "Plan", exact: true }).click();
await page.waitForFunction(() => window.__releaseDelayed);
assert.equal(await runBtn.isDisabled(), true, "Run must stay disabled while Plan is still in flight");
await page.evaluate(() => window.__releaseDelayed());
await page.locator("p.panel-summary", { hasText: "Roster 2026.09.09: 569 players" }).waitFor();
assert.equal(await runBtn.isDisabled(), false, "Run must become available once Plan has returned");
const callsAfterPlan = await page.evaluate(() => window.__calls.map((c) => c.cmd));
assert.ok(callsAfterPlan.includes("roster_plan"), "Plan must have called roster_plan");
assert.ok(!callsAfterPlan.includes("roster_run"), "Plan (always a dry run) must never itself call the run command");
console.log("ok    Roster: Run disabled until Plan returns, and Plan never calls Run");
// roster.js's own "Plan details" table is filled by its own separate
// 400ms poll of op_status (panel.js's summary line updates immediately;
// this richer table is a second reader of the same endpoint), so the
// screenshot has to wait for it too, or it would show a stale "Plan has
// not been run yet" underneath a panel that already says otherwise.
await page.getByText("Base the roster was exported from", { exact: false }).waitFor();
await shot("roster");

// 2. A refusal shows its reason and leaves Run available.
await page.goto(url + "/roster.html");
await page.evaluate(() => (window.__rosterPlanMode = "refuse"));
await page.getByRole("button", { name: "Plan", exact: true }).click();
await page.getByText("A User folder was found under this tree", { exact: false }).waitFor();
assert.equal(
  await page.getByRole("button", { name: "Run", exact: true }).isDisabled(),
  false,
  "Run must be available after a refused Plan, not left stuck disabled"
);
console.log("ok    Roster: a refusal shows its reason and leaves Run available");

// 3. A failure mid-run leaves the log visible rather than clearing it.
await page.goto(url + "/roster.html");
await page.getByRole("button", { name: "Plan", exact: true }).click();
await page.locator("p.panel-summary", { hasText: "Roster 2026.09.09: 569 players" }).waitFor();
await page.evaluate(() => (window.__rosterRunMode = "fail"));
await page.getByRole("button", { name: "Run", exact: true }).click();
await page.locator(".panel-summary.refusal", { hasText: "network unreachable" }).waitFor();
const logText = await page.locator(".panel-log").innerText();
assert.match(logText, /Hashing roster-2026\.09\.09\.json/, "the log must still show what happened before the failure");
assert.match(logText, /ERROR: network unreachable/, "the failure line itself must be in the log, not just the summary");
console.log("ok    Roster: a failure mid-run leaves the log visible");

// ---------------------------------------------------------------------------
// The Game page at 112 warnings: scrollable list, button still in the
// viewport. Measured, not eyeballed.
// ---------------------------------------------------------------------------

await page.goto(url + "/game.html");
const buildSection = page.locator("section.block").nth(0);
await buildSection.getByRole("button", { name: "Plan", exact: true }).click();
await buildSection.getByRole("button", { name: "Build anyway", exact: true }).waitFor();
await buildSection.locator(".panel-summary", { hasText: "112 problem(s) the site flagged" }).waitFor();

const list = page.locator(".capped-list");
await list.locator("li").nth(111).waitFor();
const liCount = await list.locator("li").count();
assert.equal(liCount, 112, `expected all 112 warnings in the DOM, found ${liCount}`);

const overflow = await list.evaluate((el) => ({ scrollHeight: el.scrollHeight, clientHeight: el.clientHeight }));
assert.ok(
  overflow.scrollHeight > overflow.clientHeight,
  `the warning list must overflow its box to be scrollable (scrollHeight ${overflow.scrollHeight} vs clientHeight ${overflow.clientHeight})`
);
console.log(`ok    Game: 112 warnings render, list is scrollable (${overflow.scrollHeight}px content in a ${overflow.clientHeight}px box)`);

const buildAnywayBtn = buildSection.getByRole("button", { name: "Build anyway", exact: true });
await buildAnywayBtn.waitFor();
const viewport = page.viewportSize();
const box = await buildAnywayBtn.boundingBox();
assert.ok(box, "the Build anyway button must have a real, visible bounding box");
assert.ok(box.y >= 0 && box.y + box.height <= viewport.height, `Build anyway must sit inside the viewport vertically: ${JSON.stringify(box)} vs viewport height ${viewport.height}`);
assert.ok(box.x >= 0 && box.x + box.width <= viewport.width, `Build anyway must sit inside the viewport horizontally: ${JSON.stringify(box)} vs viewport width ${viewport.width}`);
console.log(`ok    Game: Build anyway button stays fully inside the ${viewport.width}x${viewport.height} viewport at 112 warnings (box ${JSON.stringify(box)})`);
await shot("game");

// ---------------------------------------------------------------------------
// One screenshot per remaining page, the core answering everything (the
// same FULL fixture check-publisher-pages.mjs uses for its own "everything
// answered" pass), so each looks the way a healthy day looks.
// ---------------------------------------------------------------------------

for (const name of ["index", "launcher", "content", "history", "keys"]) {
  await page.goto(url + "/" + name + ".html");
  await page.waitForTimeout(150);
  await shot(name === "index" ? "status" : name);
}

assert.equal(errors.length, 0, "no page threw: " + errors.join("\n"));
console.log(`\nScreenshots written to ${out}`);
console.log("PASS: the operation panel's contract holds, and the Game page survives 112 warnings.");
await browser.close();
server.close();
