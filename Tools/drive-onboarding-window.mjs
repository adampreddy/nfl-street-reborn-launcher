// Drive the REAL launcher window through first-run setup over the WebView2
// debugging port (see drive-music-window.mjs for how to start the launcher).
// Proves the setup page cannot be left mid-operation: the logo is inert,
// Finish later is inert while the import runs, and an uninterrupted import
// carries the flow past step 2. Needs a fresh beta data root and a 2.1 install.
import { chromium } from "playwright";

const OUT = "C:/Users/Adam/.claude/jobs/f4d73a58/tmp";
const browser = await chromium.connectOverCDP("http://127.0.0.1:9333");
const context = browser.contexts()[0];
const page = context.pages().find((p) => !p.url().startsWith("devtools")) || context.pages()[0];
const base = new URL(page.url());
const t0 = Date.now();
const stamp = () => `${String(Date.now() - t0).padStart(6)} ms`;
let navs = 0;
page.on("framenavigated", (f) => { if (f === page.mainFrame()) { navs++; console.log(stamp(), "NAV", new URL(f.url()).pathname + new URL(f.url()).search); } });
const invoke = (cmd, args = {}) => page.evaluate(([c, a]) => window.__TAURI__.core.invoke(c, a), [cmd, args]);
const shot = (name) => page.screenshot({ path: `${OUT}/logo4-${name}.png` }).catch(() => {});
let failures = 0;
const check = (ok, what) => { console.log(stamp(), ok ? "PASS" : "FAIL", what); if (!ok) failures++; };

await page.goto(`${base.origin}/onboarding.html`);
await page.waitForSelector("input[name=install]", { timeout: 20000 });
const navsAtStart = navs;

// 1. The logo is not a link and clicking it goes nowhere.
const brand = page.locator(".brand");
check((await brand.evaluate((el) => el.tagName.toLowerCase())) === "span", "logo is a span, not a link");
await brand.click();
await page.waitForTimeout(1500);
check(navs === navsAtStart && page.url().endsWith("onboarding.html"), "clicking the logo on the welcome screen stays in setup");

// Choose "I already have Reborn", go to step 2.
await page.locator("input[name=install][value=migrate]").check();
await page.getByRole("button", { name: "Continue" }).click();
await page.waitForFunction(() => /Step 2/.test(document.body.textContent), null, { timeout: 30000 });
await page.waitForTimeout(1000);
const navsAtStep2 = navs;
await brand.click();
await page.waitForTimeout(1500);
check(navs === navsAtStep2 && /Step 2/.test(await page.locator("body").textContent()), "clicking the logo on step 2 stays on step 2");

// 2. Start the import; Finish later must be inert while it runs.
const exit = page.locator(".setup-exit");
check((await exit.getAttribute("aria-disabled")) !== "true", "Finish later is enabled before the import");
await page.getByRole("button", { name: "Import and continue" }).click();
await page.waitForTimeout(600);
check((await exit.getAttribute("aria-disabled")) === "true" && (await exit.evaluate((el) => el.classList.contains("disabled"))), "Finish later is disabled while the import runs");
const navsDuring = navs;
await exit.click({ force: true }).catch(() => {});
await page.keyboard.press("Enter").catch(() => {});
await page.waitForTimeout(1500);
check(navs === navsDuring && page.url().endsWith("onboarding.html"), "Finish later goes nowhere during the import");
await shot("1-importing");

// 3. Left alone, the import finishes and the flow moves past step 2.
let prog = null;
for (let i = 0; i < 150; i++) {
  await page.waitForTimeout(2000);
  prog = await invoke("onboarding_progress", { legacyRoot: null }).catch(() => null);
  if (prog?.next?.stage && prog.next.stage !== "game_verification") break;
}
console.log(stamp(), "flow now at:", JSON.stringify(prog?.next?.stage), "| h1:", (await page.locator("h1").first().textContent().catch(() => "")).trim().slice(0, 40), "| body:", (await page.locator("main").innerText().catch(() => "")).replace(/\s+/g, " ").slice(0, 120));
check(prog?.next?.stage && prog.next.stage !== "game_verification", "an uninterrupted import advances the flow past step 2");
check((await exit.getAttribute("aria-disabled")) !== "true", "Finish later is enabled again afterwards");
await shot("2-after-import");
console.log(stamp(), failures ? `${failures} FAILURE(S)` : "ALL PASS");
await browser.close();
process.exit(failures ? 1 : 0);
