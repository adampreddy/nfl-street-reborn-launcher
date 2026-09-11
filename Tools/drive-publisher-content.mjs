// Drive the REAL Reborn Publisher window through a Textures publish: Plan,
// Run, Verify, over the WebView2 debugging port, printing what the page
// shows beside what the backend's op_status says, so a gap between the two
// is pinned to a step.
//
// Start the Publisher first:
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9334"
//   & "$env:LOCALAPPDATA\Programs\Reborn Publisher\reborn-publisher.exe"
// then:
//   node Tools/drive-publisher-content.mjs [folder] [version] [--plan-only]
import { chromium } from "playwright";

const folder = process.argv[2] && !process.argv[2].startsWith("--") ? process.argv[2] : "C:\\Reborn Origin-stage\\textures-1.2.0\\GN7E69";
const version = process.argv[3] && !process.argv[3].startsWith("--") ? process.argv[3] : "1.2.0";
const planOnly = process.argv.includes("--plan-only");

const browser = await chromium.connectOverCDP("http://127.0.0.1:9334");
const context = browser.contexts()[0];
const page = context.pages().find((p) => !p.url().startsWith("devtools")) || context.pages()[0];
const base = new URL(page.url());
const t0 = Date.now();
const stamp = () => `${String(Date.now() - t0).padStart(6)} ms`;
let failures = 0;
const check = (ok, what, extra = "") => { console.log(stamp(), ok ? "PASS" : "FAIL", what, ok ? "" : extra); if (!ok) failures++; };
page.on("pageerror", (e) => console.log(stamp(), "PAGEERROR", String(e).slice(0, 300)));
page.on("console", (m) => { if (m.type() === "error" && !/ipc\.localhost|Content Security Policy|postMessage|Failed to fetch/.test(m.text())) console.log(stamp(), "CONSOLE", m.text().slice(0, 300)); });

const status = () => page.evaluate(async () => { const m = await import("./shell.js"); const r = await m.call("op_status"); return r.ok ? r.value : { phase: "status-error", summary: r.error, lines: [] }; });
const shown = async () => ({
  label: (await page.locator(".panel-phase").first().textContent().catch(() => "?")).trim(),
  summary: (await page.locator(".panel-summary").first().textContent().catch(() => "")).trim(),
  lines: ((await page.locator("pre, .panel-log").first().textContent().catch(() => "")) || "").split("\n").filter(Boolean).length,
  runDisabled: await page.getByRole("button", { name: "Run", exact: true }).isDisabled().catch(() => "?"),
});
const BUSY = new Set(["planning", "running", "verifying"]);

// Wait until the backend is no longer busy AND the page shows that; print
// backend lines as they arrive.
async function follow(what, maxSeconds) {
  let printed = 0, s = null, ui = null;
  for (let i = 0; i < maxSeconds / 3; i++) {
    await page.waitForTimeout(3000);
    s = await status(); ui = await shown();
    for (const l of (s.lines || []).slice(printed)) console.log(stamp(), "   ", l.slice(0, 150));
    printed = (s.lines || []).length;
    const settled = i > 0 && !BUSY.has(s.phase) && !/ing$/.test(ui.label.toLowerCase());
    if (i % 5 === 4 || settled) console.log(stamp(), `[${what}] backend ${s.phase} | page label ${ui.label}, ${ui.lines} lines, Run disabled ${ui.runDisabled}`);
    if (settled) break;
  }
  return { s, ui };
}

await page.goto(`${base.origin}/content.html`);
await page.waitForSelector("#ct-component", { timeout: 20000 });
await page.selectOption("#ct-component", "textures");
await page.fill("#ct-folder", folder);
await page.fill("#ct-version", version);
console.log(stamp(), "fields set:", folder, version, "| game id:", await page.locator("#ct-game-id").inputValue().catch(() => "?"));

// 1. Plan.
await page.getByRole("button", { name: "Plan", exact: true }).click();
let { s, ui } = await follow("plan", 240);
check(s.phase === "planned", "backend planned", s.phase + " " + (s.summary || ""));
check(ui.label === "Planned" && /new blob|release sequence/.test(ui.summary), "page shows the plan", `label=${ui.label} summary=${ui.summary.slice(0, 120)}`);
check(ui.runDisabled === false, "Run is enabled after the plan");
console.log(stamp(), "plan summary:", ui.summary.slice(0, 200));
if (planOnly || failures) { console.log(stamp(), failures ? `${failures} FAILURE(S)` : "PLAN OK (plan only)"); await browser.close(); process.exit(failures ? 1 : 0); }

// 2. Run, for real.
const dry = page.locator(".panel-buttons input[type=checkbox]").first();
if (await dry.isChecked()) await dry.uncheck();
check(!(await dry.isChecked()), "dry run unticked");
await page.getByRole("button", { name: "Run", exact: true }).click();
({ s, ui } = await follow("run", 900));
check(s.phase !== "planned", "the run reached the backend (an up-front refusal leaves it at planned)", ui.summary.slice(0, 160));
check(!BUSY.has(s.phase) && !/fail|refus/i.test(s.phase), "backend run finished", s.phase + " " + (s.summary || ""));
check(!/fail|refus|not configured|invalid args/i.test(ui.summary), "page shows the run result, not an error", ui.summary.slice(0, 160));
console.log(stamp(), "run summary:", (s.summary || "").slice(0, 240));

// 3. Verify.
await page.getByRole("button", { name: "Verify", exact: true }).click();
({ s, ui } = await follow("verify", 300));
check(!BUSY.has(s.phase) && !/fail|refus/i.test(s.phase), "backend verify finished", s.phase + " " + (s.summary || ""));
console.log(stamp(), "verify summary:", (s.summary || "").slice(0, 240));

console.log(stamp(), failures ? `${failures} FAILURE(S)` : "ALL PASS");
await browser.close();
process.exit(failures ? 1 : 0);
