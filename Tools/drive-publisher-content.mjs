// Drive the REAL Reborn Publisher window through a Textures plan over the
// WebView2 debugging port, and watch what the page receives. Adam saw the
// plan finish in the journal while the page stayed on "planning" with no
// Run button; this shows whether the invoke resolves, rejects, or hangs.
//
// Start the Publisher first:
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9334"
//   & "$env:LOCALAPPDATA\Programs\Reborn Publisher\reborn-publisher.exe"
// then:
//   node Tools/drive-publisher-content.mjs [folder] [version]
import { chromium } from "playwright";

const folder = process.argv[2] || "C:\\Reborn Origin-stage\\textures-1.2.0\\GN7E69";
const version = process.argv[3] || "1.2.0";
const browser = await chromium.connectOverCDP("http://127.0.0.1:9334");
const context = browser.contexts()[0];
const page = context.pages().find((p) => !p.url().startsWith("devtools")) || context.pages()[0];
const base = new URL(page.url());
const t0 = Date.now();
const stamp = () => `${String(Date.now() - t0).padStart(6)} ms`;
page.on("console", (m) => { if (["error", "warning"].includes(m.type()) && !/ipc\.localhost|Content Security Policy|postMessage/.test(m.text())) console.log(stamp(), "CONSOLE", m.type(), m.text().slice(0, 240)); });
page.on("pageerror", (e) => console.log(stamp(), "PAGEERROR", String(e).slice(0, 240)));

await page.goto(`${base.origin}/content.html`);
await page.waitForSelector("#ct-component", { timeout: 20000 });
await page.selectOption("#ct-component", "textures").catch(async () => { await page.selectOption("#ct-component", { label: "Textures" }); });
await page.fill("#ct-folder", folder);
await page.fill("#ct-version", version);
const gameId = page.locator("#ct-game-id");
if (await gameId.count()) console.log(stamp(), "game id field:", await gameId.inputValue().catch(() => "?"));
console.log(stamp(), "fields set; clicking Plan");

// Instrument the invoke so we see the promise settle, whatever the page does with it.
await page.evaluate(() => {
  const core = window.__TAURI__?.core; if (!core) return;
  const orig = core.invoke.bind(core);
  window.__seen = [];
  core.invoke = (cmd, args) => { const t = performance.now(); const p = orig(cmd, args); p.then((v) => window.__seen.push({ cmd, ms: Math.round(performance.now() - t), ok: true, v: JSON.stringify(v).slice(0, 300) }), (e) => window.__seen.push({ cmd, ms: Math.round(performance.now() - t), ok: false, e: String(e?.message || e).slice(0, 300) })); return p; };
});
await page.getByRole("button", { name: "Plan" }).click();
for (let i = 0; i < 48; i++) {
  await page.waitForTimeout(5000);
  const seen = await page.evaluate(() => window.__seen || []);
  const summary = await page.locator(".panel-summary").first().textContent().catch(() => "");
  const runDisabled = await page.getByRole("button", { name: "Run" }).isDisabled().catch(() => "?");
  const planning = /plan/i.test(summary || "") && !/blob|refus|fail|done/i.test(summary || "");
  if (i % 3 === 2 || seen.some((s) => s.cmd === "content_plan")) console.log(stamp(), "summary:", (summary || "").trim().slice(0, 160), "| Run disabled:", runDisabled);
  const done = seen.find((s) => s.cmd === "content_plan");
  if (done) { console.log(stamp(), "content_plan settled:", JSON.stringify(done).slice(0, 400)); await page.waitForTimeout(1500); console.log(stamp(), "final summary:", (await page.locator(".panel-summary").first().textContent().catch(() => "")).trim().slice(0, 200), "| Run disabled:", await page.getByRole("button", { name: "Run" }).isDisabled().catch(() => "?")); break; }
  if (!planning && i > 2) { console.log(stamp(), "page left planning without the invoke settling"); }
}
const seen = await page.evaluate(() => window.__seen || []);
console.log(stamp(), "invokes seen:", JSON.stringify(seen).slice(0, 800));
await page.screenshot({ path: "target/review/publisher-content.png" }).catch(() => {});
await browser.close();
