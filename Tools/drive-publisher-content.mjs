// Drive the REAL Reborn Publisher window through the Textures job's check:
// fill the folder and version, press "Publish textures <v>", expect the
// plain-words card with "Yes, publish" and "Cancel", then Cancel. Nothing
// is published: the check writes nothing, and this never presses Yes.
//
// Start the Publisher first:
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9334"
//   & "$env:APPDATA\NFL Street Reborn Launcher\publisher\app\reborn-publisher.exe"
// then:
//   node Tools/drive-publisher-content.mjs [folder] [version]
import { chromium } from "playwright";

const folder = process.argv[2] && !process.argv[2].startsWith("--") ? process.argv[2] : "C:\\Reborn Origin-stage\\textures-1.2.0\\GN7E69";
const version = process.argv[3] && !process.argv[3].startsWith("--") ? process.argv[3] : "1.2.1";

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

await page.goto(`${base.origin}/content.html`);
await page.waitForSelector("#ct-component", { timeout: 20000 });
await page.selectOption("#ct-component", "textures");
// The page proposes a version on its own a moment after the component
// changes; typing into the box before that lands appends to it (seen:
// "1.2.11.2.1"). Wait for the proposal, then replace it.
await page.getByText("Next:", { exact: false }).waitFor({ timeout: 20000 });
await page.fill("#ct-folder", folder);
await page.fill("#ct-version", version);
await page.locator("#ct-version").dispatchEvent("input");
await page.waitForTimeout(300);

const primary = page.getByRole("button", { name: `Publish textures ${version}`, exact: true });
check((await primary.count()) === 1, `the one button reads "Publish textures ${version}"`, (await page.locator("button").allTextContents()).join(" | "));
check((await page.getByText("Dry run").count()) === 0, "no Dry run box on the page");
check((await page.getByRole("button", { name: /^(Plan|Run|Verify)$/ }).count()) === 0, "no Plan/Run/Verify buttons on the page");

await primary.click();
console.log(stamp(), "pressed the button");
let printed = 0, s = null;
for (let i = 0; i < 100; i++) {
  await page.waitForTimeout(3000);
  s = await status();
  for (const l of (s.lines || []).slice(printed)) console.log(stamp(), "   ", l.slice(0, 150));
  printed = (s.lines || []).length;
  if (i > 0 && !["planning", "running", "verifying"].includes(s.phase)) break;
}
check(s.phase === "planned", "the backend finished the check", `${s.phase} ${s.summary || ""}`);

const yes = page.getByRole("button", { name: "Yes, publish", exact: true });
await yes.waitFor({ timeout: 10000 }).catch(() => {});
const cardText = (await page.locator(".job-card").innerText().catch(() => "")).trim();
console.log(stamp(), "card:", cardText.replace(/\n/g, " / ").slice(0, 400));
check((await yes.count()) === 1 && (await page.getByRole("button", { name: "Cancel", exact: true }).count()) >= 1, "the card offers Yes, publish and Cancel");
check(/file\(s\)/.test(cardText) && /published as textures/.test(cardText), "the card says it in plain words (files, size, version)", cardText.slice(0, 200));
check(/will download/.test(cardText), "the card says what players will download", cardText.slice(0, 200));
check(!/object\(s\)|blob|origin|manifest|sequence/i.test(cardText), "no jargon on the card", cardText.slice(0, 200));

await page.locator(".job-card").getByRole("button", { name: "Cancel", exact: true }).click();
await page.waitForTimeout(300);
check((await yes.count()) === 0 && !(await primary.isDisabled()), "Cancel puts the button back and publishes nothing");
const after = await status();
check(after.phase === "planned", "the backend never ran anything (still at the check)", after.phase);

console.log(stamp(), failures ? `${failures} FAILURE(S)` : "ALL PASS");
await browser.close();
process.exit(failures ? 1 : 0);
