// Time the Publisher's Upload plan in the REAL window: with the bucket
// listing it must say "Listed N object(s) ... in one pass" and reach a
// result in well under a minute; before, the same plan asked HEAD sixteen
// thousand times and sat for ten minutes with nothing on screen.
//
// Start the Publisher with the debugging port (see drive-publisher-content.mjs).
// `--run` then performs the upload for real (dry run OFF): whatever the plan
// found pending goes to the bucket, and the run must end with "Uploaded".
import { chromium } from "playwright";

const doRun = process.argv.includes("--run");

const browser = await chromium.connectOverCDP("http://127.0.0.1:9334");
const context = browser.contexts()[0];
const page = context.pages().find((p) => !p.url().startsWith("devtools")) || context.pages()[0];
const base = new URL(page.url());
const t0 = Date.now();
const stamp = () => `${String(Date.now() - t0).padStart(6)} ms`;
let failures = 0;
const check = (ok, what, extra = "") => { console.log(stamp(), ok ? "PASS" : "FAIL", what, ok ? "" : extra); if (!ok) failures++; };
const status = () => page.evaluate(async () => { const m = await import("./shell.js"); const r = await m.call("op_status"); return r.ok ? r.value : { phase: "status-error", summary: r.error, lines: [] }; });

await page.goto(`${base.origin}/index.html`);
await page.waitForTimeout(1500);
const answer = await page.evaluate(async () => { const m = await import("./shell.js"); return await m.call("upload_plan"); });
check(answer.ok, "upload_plan accepted", answer.error || "");
let s = null, printed = 0, listedAt = null;
for (let i = 0; i < 60; i++) {
  await page.waitForTimeout(2000);
  s = await status();
  for (const l of (s.lines || []).slice(printed)) { console.log(stamp(), "   ", l.slice(0, 150)); if (/^Listed /.test(l) && listedAt === null) listedAt = Date.now() - t0; }
  printed = (s.lines || []).length;
  if (i > 0 && !["planning", "running", "verifying"].includes(s.phase)) break;
}
console.log(stamp(), "phase:", s.phase, "| summary:", (s.summary || "").slice(0, 200));
check(listedAt !== null && listedAt < 90000, "the plan listed the bucket in one pass, within 90 s", `listed at ${listedAt} ms`);
check(!["planning", "running"].includes(s.phase), "the plan finished", s.phase);
check(/Nothing to upload|pending upload/.test(s.summary || ""), "the plan says what is pending", (s.summary || "").slice(0, 120));

if (doRun && failures === 0) {
  const t1 = Date.now();
  const run = await page.evaluate(async () => { const m = await import("./shell.js"); return await m.call("upload_run", { dry_run: false }); });
  check(run.ok, "upload_run (dry run OFF) accepted", run.error || "");
  printed = 0;
  for (let i = 0; i < 90; i++) {
    await page.waitForTimeout(2000);
    s = await status();
    for (const l of (s.lines || []).slice(printed)) console.log(stamp(), "   ", l.slice(0, 150));
    printed = (s.lines || []).length;
    if (i > 0 && !["planning", "running", "verifying"].includes(s.phase)) break;
  }
  console.log(stamp(), "phase:", s.phase, "| summary:", (s.summary || "").slice(0, 200), `| ${Date.now() - t1} ms`);
  check(/^Uploaded \d+ object\(s\)/.test(s.summary || ""), "the run uploaded what was pending", (s.summary || "").slice(0, 160));
  check(s.phase === "done", "the run finished as done", s.phase);
}
console.log(stamp(), failures ? `${failures} FAILURE(S)` : "ALL PASS");
await browser.close();
process.exit(failures ? 1 : 0);
