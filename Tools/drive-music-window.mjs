// Drive the REAL launcher window over the WebView2 debugging port: the real
// core answers every call, and the disc under the beta data root is rebuilt.
//
// Start the launcher first with the port open, on the beta channel:
//   $env:REBORN_CHANNEL="beta"; $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9333"
//   .\target\debug\reborn-launcher.exe
// then:
//   node Tools/drive-music-window.mjs <audio file>   -> target/review/window-*.png
//
// What it does: opens music.html, stages the file into slot 20 through the
// real music_stage_file command, waits for the encode, clicks the real Apply
// changes button, waits for the rebuild, screenshots, then switches the
// soundtrack off and screenshots again.
import { chromium } from "playwright";
import { mkdirSync } from "fs";

const file = process.argv[2];
if (!file) {
  console.error("give an audio file path");
  process.exit(2);
}
const OUT = "target/review";
mkdirSync(OUT, { recursive: true });

const browser = await chromium.connectOverCDP("http://127.0.0.1:9333");
const context = browser.contexts()[0];
const page = context.pages().find((p) => !p.url().startsWith("devtools")) || context.pages()[0];
console.log("attached to", page.url());
const base = new URL(page.url());
await page.goto(`${base.origin}/music.html`);
await page.waitForSelector(".music-grid .store-card", { timeout: 15000 });
await page.waitForTimeout(800);

const invoke = (cmd, args) => page.evaluate(([c, a]) => window.__TAURI__.core.invoke(c, a), [cmd, args]);
const status = async () => invoke("music_status", {});
let st = await status();
console.log("status:", st.phase, "enabled", st.enabled, "image_ok", st.image_ok, st.image_note);
if (!st.image_ok) {
  console.error("no image on this data root; seed library/imported and library/offline first");
  process.exit(1);
}

// Stage through the real command (the native file picker cannot be driven).
const inspect = await invoke("music_inspect", { path: file });
console.log("inspect:", inspect.title, "/", inspect.artist, "/", inspect.details_from, mmss(inspect.duration_ms));
st = await invoke("music_stage_file", { path: file, slot: 20, title: inspect.title || "Window test", artist: inspect.artist || "", album: inspect.album || "" });
for (let i = 0; i < 1200 && (st.phase === "encoding"); i++) {
  await page.waitForTimeout(500);
  st = await status();
}
console.log("after encode:", st.phase, st.note);
if (st.phase !== "done") {
  console.error("encode did not finish:", st.note);
  process.exit(1);
}
await invoke("music_dismiss", {});
await page.reload();
await page.waitForSelector(".music-apply", { timeout: 15000 });
await page.screenshot({ path: `${OUT}/window-music-staged.png` }).catch((e) => console.log("screenshot skipped:", e.message.slice(0, 80)));

// The real Apply button.
await page.getByRole("button", { name: "Apply changes" }).click();
st = await status();
for (let i = 0; i < 1200 && (st.phase === "building" || st.phase === "idle"); i++) {
  await page.waitForTimeout(500);
  st = await status();
}
console.log("after apply:", st.phase, st.note);
await page.waitForTimeout(1200);
await page.screenshot({ path: `${OUT}/window-music-applied.png` }).catch((e) => console.log("screenshot skipped:", e.message.slice(0, 80)));
const slot20 = st.slots.find((s) => s.slot === 20);
console.log("slot 20 on disc:", JSON.stringify(slot20));

// Switch off: the pristine disc comes back.
await invoke("music_dismiss", {});
st = await invoke("music_set_enabled", { on: false });
for (let i = 0; i < 1200 && (st.phase === "building"); i++) {
  await page.waitForTimeout(500);
  st = await status();
}
console.log("after switch off:", st.phase, st.note, "enabled", st.enabled);
await page.reload();
await page.waitForSelector(".music-grid .store-card", { timeout: 15000 });
await page.waitForTimeout(800);
await page.screenshot({ path: `${OUT}/window-music-off.png` }).catch((e) => console.log("screenshot skipped:", e.message.slice(0, 80)));
await browser.close();

function mmss(ms) {
  if (typeof ms !== "number") return "";
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
