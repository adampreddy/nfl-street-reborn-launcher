// A real browser against the real Publisher pages: Playwright, the same way
// review-onboarding.mjs drives the launcher's own pages. What the fake DOM
// in check-publisher-pages.mjs cannot see:
//
//   - The job's contract in a real layout (job.js): the primary button is
//     the only control before a click; the card with "Yes, publish" appears
//     only after the check settled, and the check never calls the run
//     command; a refusal is one red sentence with Copy; a failure mid-run
//     keeps the log behind Details; a progress bar actually has a width.
//
//   - Actual layout. The standalone Builder's window once pushed its
//     buttons off screen at 112 warnings; this drives the Build an ISO page
//     to that state and measures the confirm button's bounding box against
//     the real viewport.
//
// One screenshot per page lands in target/publisher-shots/; look at them.
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

// The same FULL fixture check-publisher-pages.mjs builds for its fake DOM.
const checkerSource = readFileSync(path.join(root, "Tools/check-publisher-pages.mjs"), "utf8");
const FULL = vm.runInNewContext(
  checkerSource.slice(checkerSource.indexOf("const FULL = {"), checkerSource.indexOf("const NULLS = ")) + "\nFULL"
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

// One fake core, shared by every page. Plain reads come out of the
// fixture; `op_status` is always answered from `window.__op`, a tiny state
// machine this script drives, since the job's contract is exactly about
// phase transitions.
await context.addInitScript((data) => {
  window.__calls = [];
  // A state planted in sessionStorage survives the page load that follows,
  // which is how a scenario starts a page with "another operation is
  // already running" rather than driving it there.
  let planted = null;
  try {
    planted = JSON.parse(sessionStorage.getItem("__op") || "null");
  } catch (_) {
    planted = null;
  }
  window.__op = planted || { phase: "idle", operation: "", lines: [], summary: "", plan: null, dry_run: false, progress: null };
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args = {}) => {
        window.__calls.push({ cmd, args });
        if (window.__delayCommand === cmd) {
          await new Promise((resolve) => {
            window.__releaseDelayed = resolve;
          });
        }
        if (cmd === "op_status") return JSON.parse(JSON.stringify(window.__op));
        if (cmd === "op_cancel") return null;
        if (cmd === "publisher_set_setting") return {};
        if (cmd === "roster_plan") {
          if (window.__rosterPlanMode === "refuse") {
            window.__op = {
              phase: "refused", operation: "roster", lines: ["Roster        C:/path/roster.rebornroster"],
              summary: "A User folder was found under this tree. Refusing to publish a scratch install by mistake.",
              plan: null, dry_run: true, progress: null,
            };
          } else {
            const summary = "Roster 2026.09.09: 569 players, 32 teams, 2,048 byte(s).";
            window.__op = {
              phase: "planned", operation: "roster",
              lines: ["Roster        C:/path/roster.rebornroster", "Read          569 players, 32 teams, 40 portrait(s)"],
              summary,
              plan: {
                summary, players: 569, teams: 32, portraits: 40, base_label: "2026.08.28", file_bytes: 2048, version: "2026.09.09",
                current_pins: [], proposed_pins: [["roster", "2026.09.09"]],
                release_bootstrap_needed: false, release_will_be_signed: true,
              },
              dry_run: true, progress: null,
            };
          }
          return { accepted: true };
        }
        if (cmd === "roster_run") {
          // Like the real command: accepted at once, the work settles later
          // (here: at once, or when the script calls window.__finishRun()).
          window.__op = { phase: "running", operation: "roster", lines: ["Publishing roster 2026.09.09..."], summary: "", plan: window.__op.plan, dry_run: !!args.dry_run, progress: { done: 1024, total: 2048, label: "Uploading" } };
          const finish = () => {
            if (window.__rosterRunMode === "fail") {
              window.__op = { ...window.__op, phase: "failed", lines: [...window.__op.lines, "Hashing roster-2026.09.09.json...", "ERROR: network unreachable"], summary: "network unreachable" };
            } else {
              window.__op = { ...window.__op, phase: "done", lines: [...window.__op.lines, "Uploaded 2 object(s)."], summary: "Published roster 2026.09.09.", progress: { done: 2048, total: 2048, label: "Uploading" } };
            }
          };
          if (window.__holdRun) window.__finishRun = finish;
          else finish();
          return { accepted: true };
        }
        if (cmd === "roster_verify") {
          window.__op = { ...window.__op, phase: "done", summary: "The server pins roster 2026.09.09." };
          return { accepted: true };
        }
        if (cmd === "game_build_plan") {
          const warnings = Array.from({ length: 112 }, (_, i) => `Player ${i + 1}: number ${(i % 99) + 1} is already worn by another player on this team.`);
          const summary = `${warnings.length} problem(s) the site flagged. Nothing was built.`;
          window.__op = {
            phase: "planned", operation: "game_build",
            lines: ["Checked base image and roster.", summary],
            summary,
            plan: { summary, warnings, players: 569, teams: 32, portraits: 40, base_label: "2026.08.28", preview_sha256: null },
            dry_run: true, progress: null,
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

const primary = () => page.getByRole("button", { name: /^Publish roster 2026/ });

// ---------------------------------------------------------------------------
// The job's contract, on the Roster page.
// ---------------------------------------------------------------------------

// 1. One button before a click; the card only once the check settled; the
//    check never calls the run command.
await page.goto(url + "/roster.html");
await primary().waitFor();
assert.equal(await page.getByRole("button", { name: "Yes, publish", exact: true }).count(), 0, "no confirm before the button is pressed");
assert.equal(await page.getByText("Dry run").count(), 0, "no Dry run box anywhere on the page");
await page.evaluate(() => (window.__delayCommand = "roster_plan"));
await primary().click();
await page.waitForFunction(() => window.__releaseDelayed);
await page.getByText("Checking...").waitFor();
assert.equal(await page.getByRole("button", { name: "Yes, publish", exact: true }).count(), 0, "no confirm while the check is in flight");
await page.evaluate(() => window.__releaseDelayed());
await page.getByText("569 players, 32 teams, 40 portraits", { exact: false }).waitFor();
const yes = page.getByRole("button", { name: "Yes, publish", exact: true });
await yes.waitFor();
const callsAfterPlan = await page.evaluate(() => window.__calls.map((c) => c.cmd));
assert.ok(callsAfterPlan.includes("roster_plan"), "the button must have called roster_plan");
assert.ok(!callsAfterPlan.includes("roster_run"), "the check must never itself call the run command");
console.log("ok    Roster: one button, then the card in plain words, and the check never ran anything");
await shot("roster-card");

// 2. Yes runs for real with a bar, then verifies on its own.
await page.evaluate(() => (window.__holdRun = true));
await yes.click();
await page.waitForFunction(() => window.__finishRun);
await page.locator(".job-bar-fill").waitFor({ state: "visible" });
const width = await page.locator(".job-bar-fill").evaluate((el) => el.getBoundingClientRect().width);
const barWidth = await page.locator(".job-bar").evaluate((el) => el.getBoundingClientRect().width);
assert.ok(width > barWidth * 0.4 && width < barWidth * 0.6, `the bar must be about half full at 1024 of 2048 bytes: ${width} of ${barWidth}`);
await page.getByText("1.00 KB of 2.00 KB (50%)", { exact: false }).waitFor();
console.log(`ok    Roster: the progress bar is ${Math.round(width)}px of ${Math.round(barWidth)}px at 50%`);
await shot("roster-publishing");
await page.evaluate(() => window.__finishRun());
await page.getByText("Done. Players now get roster 2026.09.09.", { exact: false }).waitFor();
await page.getByText("Verified from the server at", { exact: false }).waitFor();
const runCall = await page.evaluate(() => window.__calls.find((c) => c.cmd === "roster_run"));
assert.equal(runCall.args.dry_run, false, "Yes must run with dry run off");
assert.ok((await page.evaluate(() => window.__calls.map((c) => c.cmd))).includes("roster_verify"), "verify must run on its own");
console.log("ok    Roster: Yes publishes for real, verify runs on its own, the done card says what players get");
await shot("roster-done");

// 3. A refusal is one sentence with a Copy button, and the button comes back.
await page.goto(url + "/roster.html");
await page.evaluate(() => (window.__rosterPlanMode = "refuse"));
await primary().click();
await page.locator(".job-card.bad").getByText("A User folder was found under this tree", { exact: false }).waitFor();
await page.getByRole("button", { name: "Copy", exact: true }).waitFor();
assert.equal(await page.getByRole("button", { name: "Yes, publish", exact: true }).count(), 0, "no confirm after a refusal");
await page.getByRole("button", { name: "OK", exact: true }).click();
assert.equal(await primary().isDisabled(), false, "the button must be available again after a refusal");
console.log("ok    Roster: a refusal is one red sentence with Copy, and the button comes back");
await shot("roster-refused");

// 4. A failure mid-run keeps the log behind Details.
await page.goto(url + "/roster.html");
await page.evaluate(() => (window.__rosterPlanMode = "plan"));
await primary().click();
await page.getByRole("button", { name: "Yes, publish", exact: true }).click();
await page.evaluate(() => (window.__rosterRunMode = "fail"));
// The run already settled as done before the mode changed on this page load;
// run it again on a fresh load with the mode set first.
await page.goto(url + "/roster.html");
await page.evaluate(() => (window.__rosterRunMode = "fail"));
await primary().click();
await page.getByRole("button", { name: "Yes, publish", exact: true }).click();
await page.locator(".job-card.bad").getByText("network unreachable", { exact: false }).waitFor();
const details = page.locator("details.job-details");
assert.equal(await details.evaluate((d) => d.open), false, "Details stays closed by default");
await details.locator("summary").click();
const logText = await page.locator(".job-log").innerText();
assert.match(logText, /Hashing roster-2026\.09\.09\.json/, "the log must still show what happened before the failure");
assert.match(logText, /ERROR: network unreachable/, "the failure line itself must be in the log");
console.log("ok    Roster: a failure mid-run keeps the log behind Details");

// 5. Another page's running operation names that page and disables the button.
await page.goto(url + "/roster.html");
await primary().waitFor();
await page.evaluate(() => {
  sessionStorage.setItem("__op", JSON.stringify({ phase: "running", operation: "content", lines: ["Hashing..."], summary: "", plan: null, dry_run: false, progress: null }));
});
await page.goto(url + "/roster.html");
await page.getByText("Textures, runtime, mods is publishing. Open Textures, runtime, mods to watch.", { exact: true }).waitFor();
assert.equal(await primary().isDisabled(), true, "the button must be disabled while another page's operation runs");
console.log("ok    Roster: another page's running operation is named and the button is disabled");
await shot("roster-other-busy");

// 6. Another page's finished operation is ignored.
await page.evaluate(() => {
  sessionStorage.setItem("__op", JSON.stringify({ phase: "planned", operation: "content", lines: ["Listed 16,796 object(s)"], summary: "2 object(s) pending upload", plan: { summary: "x" }, dry_run: true, progress: null }));
});
await page.goto(url + "/roster.html");
await primary().waitFor();
assert.equal(await primary().isDisabled(), false);
assert.equal(await page.getByText("pending upload", { exact: false }).count(), 0, "another page's finished state must not show here");
assert.equal(await page.getByRole("button", { name: "Yes, publish", exact: true }).count(), 0);
console.log("ok    Roster: another page's finished operation is ignored");

// ---------------------------------------------------------------------------
// Build an ISO at 112 warnings: scrollable list, confirm button in the
// viewport. Measured, not eyeballed.
// ---------------------------------------------------------------------------

await page.evaluate(() => sessionStorage.removeItem("__op"));
await page.goto(url + "/game.html");
for (const [i, v] of ["C:/base.iso", "C:/roster.rebornroster", "C:/out.iso"].entries()) {
  await page.locator("section.block").nth(0).locator("input[type=text]").nth(i).fill(v);
}
await page.getByRole("button", { name: "Build the ISO", exact: true }).click();
const buildAnyway = page.getByRole("button", { name: "Build anyway", exact: true });
await buildAnyway.waitFor();
const list = page.locator(".capped-list");
await list.locator("li").nth(111).waitFor();
assert.equal(await list.locator("li").count(), 112);
const overflow = await list.evaluate((el) => ({ scrollHeight: el.scrollHeight, clientHeight: el.clientHeight }));
assert.ok(overflow.scrollHeight > overflow.clientHeight, `the warning list must overflow its box (${overflow.scrollHeight} vs ${overflow.clientHeight})`);
const viewport = page.viewportSize();
const box = await buildAnyway.boundingBox();
assert.ok(box, "Build anyway must have a real bounding box");
assert.ok(box.y >= 0 && box.y + box.height <= viewport.height, `Build anyway must sit inside the viewport vertically: ${JSON.stringify(box)}`);
assert.ok(box.x >= 0 && box.x + box.width <= viewport.width, `Build anyway must sit inside the viewport horizontally: ${JSON.stringify(box)}`);
console.log(`ok    Build an ISO: 112 warnings scroll in a ${overflow.clientHeight}px box and Build anyway stays inside ${viewport.width}x${viewport.height}`);
await shot("game");

// ---------------------------------------------------------------------------
// One screenshot per remaining page, the core answering everything.
// ---------------------------------------------------------------------------

for (const name of ["index", "launcher", "content", "history", "set", "keys"]) {
  await page.goto(url + "/" + name + ".html");
  await page.waitForTimeout(250);
  await shot(name === "index" ? "home" : name);
}

assert.equal(errors.length, 0, "no page threw: " + errors.join("\n"));
console.log(`\nScreenshots written to ${out}`);
console.log("PASS: the job's contract holds in a real browser, and Build an ISO survives 112 warnings.");
await browser.close();
server.close();
