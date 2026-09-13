// Drive the REAL Reborn Publisher window through Home's "Upload now" job
// for real: press it, expect the card, press "Yes, upload", expect the
// bar and then "Done." with the verified line. On a fully published
// origin this re-puts release.json and its signature with identical bytes
// (proven 2026-09-10), so it is safe to run against the real server.
//
// Start the Publisher with the debugging port (see drive-publisher-content.mjs).
//   node Tools/drive-publisher-upload.mjs [--check-only]
import { chromium } from "playwright";

const checkOnly = process.argv.includes("--check-only");

const browser = await chromium.connectOverCDP("http://127.0.0.1:9334");
const context = browser.contexts()[0];
const page = context.pages().find((p) => !p.url().startsWith("devtools")) || context.pages()[0];
const base = new URL(page.url());
const t0 = Date.now();
const stamp = () => `${String(Date.now() - t0).padStart(6)} ms`;
let failures = 0;
const check = (ok, what, extra = "") => { console.log(stamp(), ok ? "PASS" : "FAIL", what, ok ? "" : extra); if (!ok) failures++; };
page.on("pageerror", (e) => console.log(stamp(), "PAGEERROR", String(e).slice(0, 300)));
const status = () => page.evaluate(async () => { const m = await import("./shell.js"); const r = await m.call("op_status"); return r.ok ? r.value : { phase: "status-error", summary: r.error, lines: [] }; });

async function follow(maxSeconds) {
  let printed = 0, s = null;
  for (let i = 0; i < maxSeconds / 2; i++) {
    await page.waitForTimeout(2000);
    s = await status();
    for (const l of (s.lines || []).slice(printed)) console.log(stamp(), "   ", l.slice(0, 150));
    printed = (s.lines || []).length;
    if (i > 0 && !["planning", "running", "verifying"].includes(s.phase)) break;
  }
  return s;
}

await page.goto(`${base.origin}/index.html`);
const primary = page.getByRole("button", { name: "Upload now", exact: true });
await primary.waitFor({ timeout: 20000 });
await page.locator(".home-card h2", { hasText: "What players get right now" }).waitFor({ timeout: 20000 });
const home = (await page.locator("#screen").innerText()).replace(/\s+/g, " ");
console.log(stamp(), "home:", home.slice(0, 300));
check(/Launcher\s+\d+\.\d+\.\d+/.test(home) && /content release \d+/i.test(home), "Home shows the launcher version and the content release apart");
// The card and the pending block are this page's own words; Recent quotes
// the core's journal sentences as they were written, so it is not judged.
const own = (await page.locator(".home-card").innerText()).replace(/\s+/g, " ");
check(!/bucket|origin|pins/i.test(own), "no jargon in what players get", own.slice(0, 200));

await primary.click();
console.log(stamp(), "pressed Upload now");
let s = await follow(180);
check(s.phase === "planned", "the backend finished the check", `${s.phase} ${s.summary || ""}`);
const listed = (s.lines || []).find((l) => /^Listed /.test(l));
check(!!listed, "the check listed the server in one pass", (s.lines || []).join(" / ").slice(0, 200));

const yes = page.getByRole("button", { name: "Yes, upload", exact: true });
await yes.waitFor({ timeout: 10000 }).catch(() => {});
const cardText = (await page.locator(".job-card").innerText().catch(() => "")).trim();
console.log(stamp(), "card:", cardText.replace(/\n/g, " / ").slice(0, 300));
check((await yes.count()) === 1, "the card offers Yes, upload", cardText.slice(0, 200));

if (checkOnly) {
  await page.locator(".job-card").getByRole("button", { name: "Cancel", exact: true }).click();
  console.log(stamp(), failures ? `${failures} FAILURE(S)` : "ALL PASS (check only)");
  await browser.close();
  process.exit(failures ? 1 : 0);
}

await yes.click();
console.log(stamp(), "pressed Yes, upload");
let sawBar = false;
for (let i = 0; i < 40; i++) {
  await page.waitForTimeout(500);
  const visible = await page.locator(".job-bar").isVisible().catch(() => false);
  if (visible) { sawBar = true; console.log(stamp(), "bar:", (await page.locator(".job-bar-label").innerText().catch(() => "")).trim()); break; }
  const st = await status();
  if (!["running", "planning"].includes(st.phase)) break;
}
s = await follow(600);
check(s.phase === "done", "the backend finished the upload", `${s.phase} ${s.summary || ""}`);
console.log(stamp(), "bar seen while uploading:", sawBar, "(a 992 byte upload can finish before the first poll)");
await page.locator(".job-card.ok").waitFor({ timeout: 30000 }).catch(() => {});
const done = (await page.locator(".job-card").innerText().catch(() => "")).trim();
console.log(stamp(), "done card:", done.replace(/\n/g, " / ").slice(0, 300));
check(/^Done\./.test(done), "the done card starts with Done.", done.slice(0, 200));
check(/Verified from the server at \d/.test(done), "the done card carries the verified line with a time", done.slice(0, 200));

console.log(stamp(), failures ? `${failures} FAILURE(S)` : "ALL PASS");
await browser.close();
process.exit(failures ? 1 : 0);
