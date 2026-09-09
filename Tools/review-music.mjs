// Render the Music page in a real browser engine (headless Edge) against a
// faked core, and save screenshots to look at: the grid with one custom song
// and one staged reset, and the Add dialog after a file was inspected.
//
//     node Tools/review-music.mjs            -> target/review/music-*.png
//
// This is a visual review aid, not a pass/fail checker; the page's states are
// asserted by check-music-page.mjs.
import { chromium } from "playwright";
import { existsSync, mkdirSync, readFileSync } from "fs";

const UI = "crates/reborn-ui/ui";
const OUT = "target/review";
mkdirSync(OUT, { recursive: true });

function slot(n, extra = {}) {
  return { slot: n, title: "", artist: "", album: "", duration_ms: null, source: "original", source_name: null, pending: null, ...extra };
}
const slots = Array.from({ length: 21 }, (_, i) => slot(i + 1));
slots[20] = slot(21, { title: "Un-thinkable (I'm Ready)", artist: "Alicia Keys", album: "The Element of Freedom", duration_ms: 286_000, source: "your_file", source_name: "a1b2c3.mp3" });
slots[19] = slot(20, { title: "Ground Zero", artist: "Cole Brown", album: "Street's Disciple", duration_ms: 237_000, source: "pack", source_name: "friend.nsrmusic" });
slots[2] = slot(3, { pending: { kind: "original", title: "", artist: "", album: "" } });
const status = {
  phase: process.env.PHASE || "idle", progress: null, enabled: true, base: { kind: "original", name: null }, slots,
  pending_count: 1, note: "", image_ok: true, image_note: "", limit_seconds: 380, formats: ["mp3", "m4a", "flac", "ogg", "wav", "aiff"],
};
const inspect = {
  path: "C:\\Users\\adam\\Music\\03. Alicia Keys - Unthinkable (Official Video).mp3",
  title: "Un-thinkable (I'm Ready)", artist: "Alicia Keys", album: "The Element of Freedom",
  details_from: "lookup", duration_ms: 286_000, too_long: false, query: "Alicia Keys Unthinkable",
  candidates: [
    { title: "Un-thinkable (I'm Ready)", artist: "Alicia Keys", album: "The Element of Freedom", duration_ms: 286_000, source: "iTunes" },
    { title: "Un-thinkable (I'm Ready) [Remix]", artist: "Alicia Keys feat. Drake", album: "Single", duration_ms: 271_000, source: "iTunes" },
  ],
  note: "Details found online from the file name. Check them.",
};

const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: Number(process.env.W || 1280), height: Number(process.env.H || 900) } });
await page.route("http://music.test/**", (route) => {
  const url = new URL(route.request().url());
  const rel = decodeURIComponent(url.pathname === "/" ? "/music.html" : url.pathname);
  const file = UI + rel;
  if (!existsSync(file)) return route.fulfill({ status: 404, body: "" });
  const ct = rel.endsWith(".js") ? "text/javascript" : rel.endsWith(".css") ? "text/css" : rel.endsWith(".html") ? "text/html" : rel.endsWith(".png") ? "image/png" : rel.endsWith(".woff2") ? "font/woff2" : rel.endsWith(".svg") ? "image/svg+xml" : "application/octet-stream";
  return route.fulfill({ contentType: ct, body: readFileSync(file) });
});
await page.addInitScript(({ status, inspect }) => {
  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        switch (cmd) {
          case "music_status": return status;
          case "pick_path": return args && args.kind === "audio" ? inspect.path : null;
          case "music_inspect": return inspect;
          case "music_lookup": return inspect.candidates;
          default: return status;
        }
      },
    },
  };
}, { status, inspect });
await page.goto("http://music.test/music.html");
await page.waitForTimeout(600);
await page.screenshot({ path: `${OUT}/music-grid.png`, fullPage: true });
console.log("screen:", await page.evaluate(() => { const s = getComputedStyle(document.getElementById("screen")); const m = getComputedStyle(document.querySelector("main")); return { screenDisplay: s.display, screenCols: s.gridTemplateColumns, screenFlex: s.flexDirection, screenWrap: s.flexWrap, mainDisplay: m.display, mainCols: m.gridTemplateColumns, mainFlex: m.flexDirection, mainWrap: m.flexWrap, screenWidth: document.getElementById("screen").clientWidth }; }));
if (process.env.RULES) {
  console.log("rules:", await page.evaluate(() => {
    const el = document.getElementById("screen");
    const out = [];
    for (const ss of document.styleSheets) {
      let rules = [];
      try { rules = [...ss.cssRules]; } catch (_) { continue; }
      const walk = (list, media) => { for (const r of list) { if (r.cssRules && r.media) walk([...r.cssRules], r.media.mediaText); else if (r.selectorText && r.style && (r.style.display || r.style.gridTemplateColumns)) { try { if (el.matches(r.selectorText)) out.push(`${ss.href?.split("/").pop()} ${media ? "@" + media + " " : ""}${r.selectorText} {display:${r.style.display}; cols:${r.style.gridTemplateColumns}}`); } catch (_) {} } } };
      walk(rules, "");
    }
    return out;
  }));
}
await page.getByRole("button", { name: "Add a song", exact: true }).click();
await page.waitForTimeout(600);
await page.screenshot({ path: `${OUT}/music-add.png`, fullPage: true });
await browser.close();
console.log(`wrote ${OUT}/music-grid.png and ${OUT}/music-add.png`);
