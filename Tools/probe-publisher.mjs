// Watch the Publisher's Content page through one Plan, sampling what the
// panel shows and what the page's own `call("op_status")` answers, so the
// gap between the backend (which finishes) and the page (which stays on
// "Planning") is pinned to a line.
import { chromium } from "playwright";
const browser = await chromium.connectOverCDP("http://127.0.0.1:9334");
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const t0 = Date.now();
const stamp = () => `${String(Date.now() - t0).padStart(6)} ms`;
page.on("pageerror", (e) => console.log(stamp(), "PAGEERROR", String(e).slice(0, 300)));
page.on("console", (m) => { if (m.type() === "error" && !/ipc\.localhost|Content Security Policy|postMessage/.test(m.text())) console.log(stamp(), "CONSOLE", m.text().slice(0, 300)); });

const sample = async (label) => {
  const phase = (await page.locator(".panel-phase").first().textContent().catch(() => "?")).trim();
  const note = (await page.locator(".panel-note, .busy-note, [class*=busy]").first().textContent().catch(() => "")).trim();
  const summary = (await page.locator(".panel-summary").first().textContent().catch(() => "")).trim();
  const lines = ((await page.locator("pre, .panel-log").first().textContent().catch(() => "")) || "").split("\n").filter(Boolean).length;
  const run = await page.getByRole("button", { name: "Run" }).isDisabled().catch(() => "?");
  const status = await page.evaluate(async () => { try { const m = await import("./shell.js"); const r = await m.call("op_status"); return JSON.stringify({ ok: r.ok, phase: r.value?.phase, lines: r.value?.lines?.length, error: r.error }); } catch (e) { return "call threw " + String(e?.message || e).slice(0, 120); } });
  console.log(stamp(), label, "| label:", phase, "| log lines:", lines, "| run disabled:", run, "| note:", note.slice(0, 80), "| summary:", summary.slice(0, 60), "| op_status:", status);
};

await sample("before");
await page.getByRole("button", { name: "Plan" }).click();
for (let i = 0; i < 40; i++) { await page.waitForTimeout(3000); await sample(`t+${(i + 1) * 3}s`); const st = await page.evaluate(async () => { const m = await import("./shell.js"); const r = await m.call("op_status"); return r.value?.phase; }); if (i > 2 && st && st !== "planning") { await page.waitForTimeout(4000); await sample("after"); break; } }
await browser.close();
