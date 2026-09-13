// Drive the REAL launcher window's Mods page over the WebView2 debugging port:
// switch the NFL Street 1 soundtrack package on (the real core rebuilds the
// disc), check the Music page, then switch the original back on.
//
//   node Tools/drive-mods-window.mjs   -> target/review/window-mods-*.png
import { chromium } from "playwright";
import { mkdirSync } from "fs";

const OUT = "target/review";
mkdirSync(OUT, { recursive: true });
const browser = await chromium.connectOverCDP("http://127.0.0.1:9333");
const page = browser.contexts()[0].pages()[0];
const origin = new URL(page.url()).origin;
const invoke = (cmd, args) => page.evaluate(([c, a]) => window.__TAURI__.core.invoke(c, a), [cmd, args]);
const shot = (name) => page.screenshot({ path: `${OUT}/${name}.png` }).catch((e) => console.log("screenshot skipped:", e.message.slice(0, 80)));

async function soundtrackTab() {
  await page.goto(`${origin}/mods.html`);
  await page.waitForSelector(".section-tabs", { timeout: 15000 });
  await page.getByRole("button", { name: "Soundtrack", exact: true }).click();
  await page.waitForTimeout(400);
}

async function toggle(name, expectText) {
  const button = page.locator(`button[aria-label="Enable ${name}"]`);
  if ((await button.count()) === 0) {
    console.log(`no Enable button for ${name} (already enabled?)`);
    return;
  }
  await button.first().click();
  await page.waitForFunction((t) => document.body.innerText.includes(t), expectText, { timeout: 600000 });
  await page.waitForTimeout(500);
}

await soundtrackTab();
const lib = await invoke("mod_packages", {});
console.log("soundtrack packages:", lib.packages.filter((p) => p.kind === "soundtrack").map((p) => `${p.id} active=${p.active}`).join(", "));
console.log("unreadable:", lib.unreadable.length);
await shot("window-mods-soundtrack-tab");

await toggle("NFL Street 1 Soundtrack", "custom song");
console.log("after enable:", (await page.locator(".partial").allInnerTexts()).join(" | "));
await shot("window-mods-ns1-enabled");
let st = await invoke("music_status", {});
const fromMod = st.slots.filter((s) => s.source === "soundtrack_mod");
console.log("music_status: base", JSON.stringify(st.base), "soundtrack_mod slots", fromMod.length, "first:", fromMod[0] && `${fromMod[0].title} / ${fromMod[0].artist}`);
await page.goto(`${origin}/music.html`);
await page.waitForSelector(".music-grid .store-card", { timeout: 15000 });
await page.waitForTimeout(800);
await shot("window-music-ns1");

await soundtrackTab();
await toggle("NFL Street Reborn Soundtrack", "its own soundtrack");
console.log("after reborn:", (await page.locator(".partial").allInnerTexts()).join(" | "));
st = await invoke("music_status", {});
console.log("music_status: base", JSON.stringify(st.base), "soundtrack_mod slots", st.slots.filter((s) => s.source === "soundtrack_mod").length, "enabled", st.enabled);
await shot("window-mods-reborn-enabled");
await browser.close();
