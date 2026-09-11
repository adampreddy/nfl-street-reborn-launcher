// Press Run, then Verify, on the Publisher's Content page in the REAL window,
// and print the operation's own lines as it goes. Expects a plan to be
// showing already and the dry-run box unticked (it refuses otherwise).
import { chromium } from "playwright";

const browser = await chromium.connectOverCDP("http://127.0.0.1:9334");
const page = browser.contexts()[0].pages().find((p) => !p.url().startsWith("devtools"));
const t0 = Date.now();
const stamp = () => `${String(Date.now() - t0).padStart(6)} ms`;
const status = async () => page.evaluate(async () => { const m = await import("./shell.js"); const r = await m.call("op_status"); return r.ok ? r.value : { phase: "status-error", summary: r.error, lines: [] }; });
const busy = (s) => ["planning", "running", "verifying"].includes(s?.phase);

const dry = await page.locator(".panel-buttons input[type=checkbox]").first().isChecked();
if (dry) { console.log("dry run is ticked; unticking it"); await page.locator(".panel-buttons input[type=checkbox]").first().uncheck(); }
let s = await status();
console.log(stamp(), "before: phase", s.phase, "| Run disabled:", await page.getByRole("button", { name: "Run" }).isDisabled());
if (s.phase !== "planned") { console.log("no plan showing; plan first"); process.exit(1); }

await page.getByRole("button", { name: "Run" }).click();
let printed = 0;
for (let i = 0; i < 120; i++) {
  await page.waitForTimeout(5000);
  s = await status();
  const lines = s.lines || [];
  for (const l of lines.slice(printed)) console.log(stamp(), "  ", l.slice(0, 150));
  printed = lines.length;
  if (i > 0 && !busy(s)) break;
}
console.log(stamp(), "run ended: phase", s.phase, "| summary:", (s.summary || "").slice(0, 240));
if (s.phase === "ran" || s.phase === "done" || /ran|done|published/i.test(s.phase)) {
  await page.getByRole("button", { name: "Verify" }).click();
  printed = (await status()).lines?.length || 0;
  for (let i = 0; i < 60; i++) {
    await page.waitForTimeout(5000);
    s = await status();
    const lines = s.lines || [];
    for (const l of lines.slice(printed)) console.log(stamp(), "  ", l.slice(0, 150));
    printed = lines.length;
    if (i > 0 && !busy(s)) break;
  }
  console.log(stamp(), "verify ended: phase", s.phase, "| summary:", (s.summary || "").slice(0, 240));
}
await browser.close();
