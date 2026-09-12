// Drive the REAL launcher window through the 2026-09-12 layout: tuck and
// adopt observed on disk, "I already have Reborn" from the tucked Reborn 2.1,
// the wizard through to Play, prepare_launch and a sync, then the readbacks
// the spec asks for, a real launch, two screenshots, a graceful close, and
// Dolphin.ini read back after the game wrote its own config.
//
// Started by Tools\walk-paths-overhaul.ps1, which sets REBORN_WALK_ROOT (the
// scratch beta data root), REBORN_WALK_OLD (the Launcher-era root) and
// REBORN_WALK_OUT (screenshots), and opens the debugging port on 9333.
import { chromium } from "playwright";
import { mkdirSync, existsSync, readFileSync, readdirSync, lstatSync, statSync } from "fs";
import { join } from "path";
import { execSync } from "child_process";

const ROOT = process.env.REBORN_WALK_ROOT;
const OLD = process.env.REBORN_WALK_OLD;
const OUT = process.env.REBORN_WALK_OUT || "target/review";
mkdirSync(OUT, { recursive: true });
if (!ROOT || !OLD) { console.log("REBORN_WALK_ROOT and REBORN_WALK_OLD are required"); process.exit(2); }

const browser = await chromium.connectOverCDP("http://127.0.0.1:9333");
const context = browser.contexts()[0];
const page = context.pages().find((p) => !p.url().startsWith("devtools")) || context.pages()[0];
const base = new URL(page.url());
const t0 = Date.now();
const stamp = () => `${String(Date.now() - t0).padStart(6)} ms`;
let failures = 0;
const check = (ok, what, extra = "") => { console.log(stamp(), ok ? "PASS" : "FAIL", what, ok ? "" : String(extra).slice(0, 300)); if (!ok) failures++; };
const invoke = (cmd, args = {}) => page.evaluate(([c, a]) => window.__TAURI__.core.invoke(c, a), [cmd, args]);
const shot = (name) => page.screenshot({ path: `${OUT}/paths-${name}.png` }).catch(() => {});
const stage = async () => (await invoke("onboarding_progress", { legacyRoot: null }).catch(() => null))?.next?.stage;
const untilStage = async (want, ms) => { for (let i = 0; i < ms / 500; i++) { if ((await stage()) === want) return true; await page.waitForTimeout(500); } return false; };
const next = () => page.locator("#next");
const clickNext = async () => { await page.waitForFunction(() => { const b = document.getElementById("next"); return b && !b.disabled; }, null, { timeout: 60000 }); await next().click(); };
const countFiles = (dir) => { let n = 0; for (const e of readdirSync(dir, { withFileTypes: true })) { if (e.isDirectory()) n += countFiles(join(dir, e.name)); else n++; } return n; };
const iniHas = (text, key) => new RegExp("^\\s*" + key + "\\s*=", "m").test(text);
const PATH_KEYS = ["LoadPath", "DumpPath", "NANDRootPath", "ResourcePackPath", "MemcardAPath", "MemcardBPath", "DefaultISO", "ISOPath0", "ISOPaths"];
const ps = (script) => execSync('powershell -NoProfile -Command "' + script.replace(/"/g, '\\"') + '"').toString().trim();
const dolphinCount = () => Number(ps("(Get-Process -Name 'NFL Street Reborn Dolphin' -ErrorAction SilentlyContinue | Measure-Object).Count") || 0);
const screen = (file) => {
  const png = join(OUT, file).replace(/\\/g, "/");
  ps("Add-Type -AssemblyName System.Windows.Forms,System.Drawing; $b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds; $bmp=New-Object System.Drawing.Bitmap $b.Width,$b.Height; $g=[System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Location,[System.Drawing.Point]::Empty,$b.Size); $bmp.Save('" + png + "'); $g.Dispose(); $bmp.Dispose()");
  console.log(stamp(), "screen captured to", png);
};
const hardlinksOf = (file) => ps("fsutil hardlink list '" + file.replace(/'/g, "''") + "'").split(/\r?\n/).map((l) => l.trim()).filter(Boolean);

// 0. Before the window did anything else: the 2.1 install was tucked, the
// Launcher-era root adopted. Both happened at process start.
check(existsSync(join(ROOT, "Reborn 2.1", "User", "GC", "MemoryCardA.USA.raw")), "the 2.1 User folder was tucked into Reborn 2.1");
check(existsSync(join(ROOT, "Reborn 2.1", "Games", "NSR.iso")), "the 2.1 disc was tucked with it");
check(existsSync(join(ROOT, "Reborn 2.1", "MODS", "!BACKUP")), "JSGME's MODS entries were tucked");
check(!existsSync(join(ROOT, "User")) && !existsSync(join(ROOT, "Games")), "the data root no longer holds the 2.1 entries");
check(existsSync(join(ROOT, "library", "adopted-marker.txt")), "the Launcher-era library was adopted by rename");
check(!existsSync(join(OLD, "library")), "and it left the old root");
check(existsSync(join(ROOT, "config", "settings.json")), "the Launcher-era settings came along");

// 1. First run: I already have Reborn. The tucked folder is found by itself.
await page.goto(`${base.origin}/onboarding.html`);
await page.waitForSelector("input[name=install]", { timeout: 20000 });
console.log(stamp(), "welcome; stage =", await stage());
await page.locator("input[name=install][value=migrate]").check();
await clickNext();
check(await untilStage("game_verification", 20000), "reached the installation step");
await page.waitForTimeout(2500);
const found = (await page.locator("main").innerText().catch(() => "")).replace(/\s+/g, " ");
check(/INSTALLATION FOUND/i.test(found), "the installation was found without browsing", found);
check(/Reborn 2\.1/.test(found), "and it is the tucked Reborn 2.1 folder", found);
check(/NSR\.iso/.test(found), "with the 2.1 disc in it", found);
check(/Existing saves found/.test(found), "and the memory card", found);
await shot("1-found");

// 2. Import and continue: saves and settings copied, the disc patched to the
// newest build by the shipped chain.
await page.getByRole("button", { name: "Import and continue" }).click();
let prog = null;
for (let i = 0; i < 240; i++) {
  await page.waitForTimeout(2000);
  prog = await invoke("onboarding_progress", { legacyRoot: null }).catch(() => null);
  if (prog?.next?.stage && prog.next.stage !== "game_verification") break;
}
check(prog?.next?.stage && prog.next.stage !== "game_verification", "the import finished and the flow moved on", JSON.stringify(prog?.next));
await shot("2-imported");

// 3. The rest of the wizard, whatever stages this flow has: choose "none" for
// the controller, the first option elsewhere, until completion.
for (let i = 0; i < 14; i++) {
  const s = await stage();
  console.log(stamp(), "stage:", s);
  if (!s || s === "completion") break;
  if (s === "controller_setup") {
    await page.waitForSelector("select[aria-label='Your controller']", { timeout: 20000 });
    await page.locator("select[aria-label='Your controller']").selectOption("none").catch(() => {});
    await page.waitForTimeout(1500);
  } else {
    const radio = page.locator("main input[type=radio]").first();
    if ((await radio.count()) && !(await page.locator("main input[type=radio]:checked").count())) await radio.check();
  }
  await clickNext();
  for (let j = 0; j < 480; j++) { await page.waitForTimeout(500); if ((await stage()) !== s) break; }
}
check((await stage()) === "completion", "preparation finished", await stage());
await shot("5-completion");
await clickNext();
await page.waitForURL(/index\.html/, { timeout: 60000 }).catch(() => {});
check(page.url().includes("index.html"), "setup finished and landed on Play", page.url());
await page.waitForTimeout(3000);
await shot("6-play");

// 4. Prepare a launch and sync the mods the way a launch does: the readbacks.
let userDir = null;
try {
  const prep = await invoke("prepare_launch", { runtime: null, game: null, preset: "balanced", fullscreen: false, userDir: null });
  userDir = prep.user_dir;
  console.log(stamp(), "user dir:", userDir);
  check(typeof userDir === "string" && userDir.replace(/\\/g, "/").includes("/dolphin-users/migrated-"), "the launch uses the migrated user folder", userDir);
} catch (e) { check(false, "prepare_launch ran", String(e.message || e)); }
try {
  const sync = await invoke("mods_sync", { runtime: null });
  console.log(stamp(), "mods_sync:", JSON.stringify(sync).slice(0, 240));
  check(sync && sync.synced === true, "the texture sync composed into the user folder", JSON.stringify(sync).slice(0, 200));
} catch (e) { check(false, "mods_sync ran", String(e.message || e)); }
if (userDir) {
  const iniPath = join(userDir, "Config", "Dolphin.ini");
  const ini = readFileSync(iniPath, "utf8");
  for (const k of PATH_KEYS) check(!iniHas(ini, k), `Dolphin.ini carries no ${k}`);
  check(/^\s*SlotA\s*=\s*1\s*$/m.test(ini), "slot A is a memory card at Dolphin's default place");
  check(existsSync(join(userDir, "Config", "Dolphin.ini.before-path-law")), "the 2.1 Dolphin.ini was kept once before the law was applied");
  check(existsSync(join(userDir, "GC", "MemoryCardA.USA.raw")), "the memory card came along into the user folder");
  check(existsSync(join(userDir, "Cache", "GN7E69.uidcache")), "the template's shader list was seeded");
  const tex = join(userDir, "Load", "Textures", "GN7E69");
  const real = existsSync(tex) && !lstatSync(tex).isSymbolicLink() && statSync(tex).isDirectory();
  check(real, "Load/Textures/GN7E69 is a real directory, not a link");
  check(real && existsSync(join(tex, "composed.json")), "and it is the composed tree");
  const n = real ? countFiles(tex) : 0;
  check(n > 8000, `it holds the whole pack (${n} entries)`);
  const marker = real && existsSync(join(tex, "composed.json")) ? JSON.parse(readFileSync(join(tex, "composed.json"), "utf8")) : null;
  console.log(stamp(), "composed record identity:", JSON.stringify(marker?.record?.pristine_identity ?? null), "pristine:", marker?.record?.pristine_root);
  // One composed texture must be ONE file with three names: the payload's
  // copy beside the launcher, the pristine tree in library, and the composed
  // tree in the user folder. Copies would cost 6.5 GB per user folder.
  if (real) {
    const sample = readdirSync(tex).find((f) => /\.(dds|png)$/i.test(f));
    const links = sample ? hardlinksOf(join(tex, sample)) : [];
    console.log(stamp(), "hardlinks of", sample, "->", links.length, JSON.stringify(links).slice(0, 400));
    check(links.length >= 3 && links.some((l) => /\\library\\textures\\/i.test(l)) && links.some((l) => /\\content\\textures\\/i.test(l)), "the composed texture is a hardlink shared with the pristine tree and the payload", links.join(" | "));
  }
  check(!existsSync(join(ROOT, "mods", "active")), "no mods/active under the data root");
  check(!existsSync(join(userDir, "Load", "Textures", "GN7E69.building")) && !existsSync(join(userDir, "Load", "Textures", "GN7E69.previous")), "no build leftovers beside it");
  const tuckedIni = join(ROOT, "Reborn 2.1", "User", "Config", "Dolphin.ini");
  check(existsSync(tuckedIni) && iniHas(readFileSync(tuckedIni, "utf8"), "LoadPath"), "the tucked 2.1 Dolphin.ini is untouched");
}

// 5. A real launch. `launch` waits for the game to end, so it is fired and
// left running while the screen is captured twice (the shader compile from
// the seeded list comes first, then the game) and the game closed gracefully.
const launching = invoke("launch", { runtime: null, game: null, preset: "balanced", fullscreen: false, userDir: null, modsDisabled: false, leagueMatch: null })
  .then((r) => ({ ok: true, r })).catch((e) => ({ ok: false, error: String(e.message || e) }));
let dolphinSeen = false;
for (let i = 0; i < 60; i++) { await page.waitForTimeout(1000); if (dolphinCount() > 0) { dolphinSeen = true; break; } }
check(dolphinSeen, "Dolphin started");
if (dolphinSeen) {
  await page.waitForTimeout(40000);
  screen("paths-7-game-40s.png");
  await page.waitForTimeout(60000);
  screen("paths-8-game-100s.png");
  ps("Get-Process -Name 'NFL Street Reborn Dolphin' -ErrorAction SilentlyContinue | ForEach-Object { $null = $_.CloseMainWindow() }");
  let closed = false;
  for (let i = 0; i < 90; i++) { await page.waitForTimeout(1000); if (dolphinCount() === 0) { closed = true; break; } }
  if (!closed) {
    console.log(stamp(), "Dolphin ignored the close for 90 s; asking once more, then stopping it");
    ps("Get-Process -Name 'NFL Street Reborn Dolphin' -ErrorAction SilentlyContinue | ForEach-Object { $null = $_.CloseMainWindow() }");
    for (let i = 0; i < 30; i++) { await page.waitForTimeout(1000); if (dolphinCount() === 0) { closed = true; break; } }
    if (!closed) ps("Get-Process -Name 'NFL Street Reborn Dolphin' -ErrorAction SilentlyContinue | Stop-Process -Force");
  }
  check(closed, "Dolphin closed on request (a forced stop skips its config write)");
  const result = await Promise.race([launching, page.waitForTimeout(60000).then(() => ({ ok: false, error: "launch did not return within 60 s of the game closing" }))]);
  console.log(stamp(), "launch result:", JSON.stringify(result).slice(0, 300));
  check(result.ok, "the launch returned after the game closed", result.error);
  if (userDir) {
    const after = readFileSync(join(userDir, "Config", "Dolphin.ini"), "utf8");
    const wrote = PATH_KEYS.filter((k) => iniHas(after, k));
    console.log(stamp(), "path keys Dolphin wrote back on exit:", JSON.stringify(wrote));
    check(wrote.length === 0, "after the game exited, Dolphin.ini still carries no path keys (the fork writes none back)", wrote.join(", "));
    check(existsSync(join(userDir, "GC", "MemoryCardA.USA.raw")), "the memory card is still in the user folder");
    const tex = join(userDir, "Load", "Textures", "GN7E69");
    check(existsSync(join(tex, "composed.json")), "the composed tree is still in place after the game");
  }
}

console.log(stamp(), failures ? `${failures} FAILURE(S)` : "ALL PASS");
await browser.close();
process.exit(failures ? 1 : 0);
