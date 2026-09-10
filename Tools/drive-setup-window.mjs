// Walk first-run setup in the REAL launcher window, "Start fresh", with the
// game the installer carries, then look at what beta.4 changed: the game is
// imported without a disc, choosing a controller is enough to continue,
// Stats is a page, the Mods library is full, Settings offers patch notes and
// uninstall, the Play page says "Ready to play".
//
// Start the launcher first with the debugging port open, on a FRESH beta data
// root (see drive-music-window.mjs for the environment variables), from the
// full installer payload so content\games and content\mods are beside it:
//   .\target\installer-payload\reborn-launcher.exe
// then:
//   node Tools/drive-setup-window.mjs   -> screenshots in target/review
import { chromium } from "playwright";
import { mkdirSync } from "fs";

const OUT = "target/review";
mkdirSync(OUT, { recursive: true });
const browser = await chromium.connectOverCDP("http://127.0.0.1:9333");
const context = browser.contexts()[0];
const page = context.pages().find((p) => !p.url().startsWith("devtools")) || context.pages()[0];
const base = new URL(page.url());
const t0 = Date.now();
const stamp = () => `${String(Date.now() - t0).padStart(6)} ms`;
let failures = 0;
const check = (ok, what, extra = "") => { console.log(stamp(), ok ? "PASS" : "FAIL", what, ok ? "" : extra); if (!ok) failures++; };
const invoke = (cmd, args = {}) => page.evaluate(([c, a]) => window.__TAURI__.core.invoke(c, a), [cmd, args]);
const shot = (name) => page.screenshot({ path: `${OUT}/setup-${name}.png` }).catch(() => {});
const stage = async () => (await invoke("onboarding_progress", { legacyRoot: null }).catch(() => null))?.next?.stage;
const untilStage = async (want, ms) => { for (let i = 0; i < ms / 500; i++) { if ((await stage()) === want) return true; await page.waitForTimeout(500); } return false; };
const next = () => page.locator("#next");
const clickNext = async () => { await page.waitForFunction(() => { const b = document.getElementById("next"); return b && !b.disabled; }, null, { timeout: 60000 }); await next().click(); };

await page.goto(`${base.origin}/onboarding.html`);
await page.waitForSelector("input[name=install]", { timeout: 20000 });
console.log(stamp(), "welcome; stage =", await stage());

// 1. Start fresh.
await page.locator("input[name=install][value=fresh]").check();
await clickNext();
check(await untilStage("game_verification", 20000), "reached game verification");

// 2. The shipped game is prepared by itself; no disc is asked for.
// The backend reaches the stage a beat before the page draws it, so wait for
// the drawn step before reading its copy.
await page.waitForFunction(() => /NFL Street Reborn is ready/.test(document.body.textContent), null, { timeout: 180000 }).catch(() => {});
const intro = await page.locator("main").innerText();
check(/included with this install/i.test(intro || ""), "game step says the game is included", intro.slice(0, 160));
const status = await page.locator(".import-status").first().textContent().catch(() => "");
check(/NFL Street Reborn is ready/.test(status || ""), "shipped game imported without browsing", status);
await shot("2-game");
await clickNext();
check(await untilStage("controller_setup", 20000), "reached the controller step");

// 3. Choose a controller from the list; Continue must enable without Scan again.
await page.waitForSelector("select[aria-label='Your controller']", { timeout: 20000 });
const select = page.locator("select[aria-label='Your controller']");
const options = await select.locator("option").evaluateAll((os) => os.map((o) => o.value));
const real = options.find((v) => v !== "" && v !== "none");
await select.selectOption(real ?? "none");
console.log(stamp(), "controller options:", JSON.stringify(options), "chose", real ?? "none");
await page.waitForTimeout(2500);
check(!(await next().isDisabled()), real ? "choosing a controller enables Continue without Scan again" : "declining a controller enables Continue");
await shot("3-controller");
await clickNext();
check(await untilStage("controls_profile", 20000), "reached controls profile");

// 4. The remaining choices: first option each, defaults where preselected.
for (const [name, want] of [["controls_profile", "system_check"], ["system_check", "updates"], ["updates", "preparation"]]) {
  const radio = page.locator("main input[type=radio]").first();
  if (await radio.count() && !(await page.locator("main input[type=radio]:checked").count())) await radio.check();
  await clickNext();
  check(await untilStage(want, 60000), `${name} -> ${want}`);
}
// Preparation runs its tasks, then completion.
await clickNext();
check(await untilStage("completion", 240000), "preparation finished");
await shot("5-completion");
await clickNext();
await page.waitForURL(/index\.html/, { timeout: 60000 }).catch(() => {});
check(page.url().includes("index.html"), "setup finished and landed on Play", page.url());
await page.waitForTimeout(3000);
await shot("6-play");
const play = await page.locator("main").innerText().catch(() => "");
check(/Ready to play/.test(play), "Play says Ready to play before the first game");
check(/track your stats from your games/.test(play), "sign-in copy updated");

// 4b. Prepare a launch without launching: the new user folder must start from
// the runtime's Dolphin baseline, with the launcher's required keys on top.
try {
  const prep = await invoke("prepare_launch", { runtime: null, game: null, preset: "balanced", fullscreen: false, userDir: null });
  const touched = [...(prep.updated || []), ...(prep.unchanged || [])].map((s) => String(s).replace(/\\/g, "/"));
  check(touched.some((f) => /Config\/Dolphin\.ini$/.test(f)) && touched.some((f) => /GameSettings\/GN7E69\.ini$/.test(f)), "prepare wrote the Dolphin config", touched.slice(0, 6).join(" | "));
  console.log(stamp(), "user dir:", prep.user_dir, "| updated:", (prep.updated || []).length, "unchanged:", (prep.unchanged || []).length);
  console.log("USERDIR " + prep.user_dir);
} catch (e) { check(false, "prepare_launch ran", String(e.message || e).slice(0, 200)); }

// 5. The new page and the fuller library.
await page.goto(`${base.origin}/stats.html`);
await page.waitForFunction(() => /Game results/.test(document.body.textContent), null, { timeout: 20000 }).catch(() => {});
check(/Account/.test(await page.locator("main").innerText()) && /Send results now/.test(await page.locator("main").innerText()), "Stats page draws account and results");
check((await page.locator(".shell-nav a[href='stats.html']").count()) === 1, "sidebar has a Stats entry");
await shot("7-stats");
await page.goto(`${base.origin}/mods.html`);
await page.waitForTimeout(4000);
const modsText = await page.locator("main").innerText().catch(() => "");
const packs = (modsText.match(/football|throwback|alternate/gi) || []).length;
check(packs >= 5, `Mods library lists the shipped packs (${packs} matches)`);
await shot("8-mods");
await page.goto(`${base.origin}/settings.html#updates`);
await page.waitForTimeout(3000);
const settingsText = await page.locator("main").innerText().catch(() => "");
check(/Patch notes/.test(settingsText) && /Uninstall NFL Street Reborn/.test(settingsText), "Settings offers patch notes and uninstall");
await shot("9-settings");

console.log(stamp(), failures ? `${failures} FAILURE(S)` : "ALL PASS");
await browser.close();
process.exit(failures ? 1 : 0);
