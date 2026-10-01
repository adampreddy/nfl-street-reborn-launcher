// Every product page gets the new look the same way, and theme.css holds the rules.
//     node Tools/check-theme.mjs
import { readFileSync, existsSync } from "node:fs";
import assert from "node:assert/strict";
const ui = "crates/reborn-ui/ui/";
const PAGES = ["index","controls","game","live","mods","music","onboarding","online","profile","recovery","settings","stats"];
// Pages moved onto the design system load exactly theme.css + their own page file (spec §11) and say so
// with body.themed. Later tasks add pages here as they move.
const THEMED = { index: "pages/play.css", settings: "pages/settings.css", mods: "pages/mods.css", music: "pages/music.css", controls: "pages/controls.css", stats: "pages/stats.css", profile: "pages/profile.css",
  recovery: "pages/recovery.css", game: "pages/live.css", live: "pages/live.css", online: "pages/online.css",
};
for (const p of PAGES) {
  const html = readFileSync(ui + p + ".html", "utf8");
  const head = html.slice(0, html.indexOf("</head>"));
  assert.match(head, /<script src="look-boot\.js"><\/script>/, `${p}: look-boot.js must load in <head>, classic and synchronous`);
  const sheets = [...head.matchAll(/href="([^"]+\.css)"/g)].map(m => m[1]);
  if (THEMED[p]) {
    assert.deepEqual(sheets, ["theme.css", THEMED[p]], `${p}: must load exactly theme.css + ${THEMED[p]} (got ${sheets.join(", ")})`);
    assert.match(html, /<body class="[^"]*\bthemed\b/, `${p}: body must carry the themed class`);
    assert.ok(existsSync(ui + THEMED[p]), `${p}: ${THEMED[p]} must exist`);
  } else {
    assert.equal(sheets.at(-1), "theme.css", `${p}: theme.css must be the LAST stylesheet (got ${sheets.join(", ")})`);
  }
  assert.match(html, /<span id="input-hints"><\/span>/, `${p}: #input-hints starts empty (the hint bar exists only while a prompt is shown)`);
  assert.doesNotMatch(html, /<script>(?!<\/script>)/, `${p}: no inline scripts (CSP script-src 'self')`);
}
const theme = readFileSync(ui + "theme.css", "utf8");
for (const t of ["--mica-tint","--glass-1","--glass-2","--glass-3","--stroke","--stroke-hi","--sheen","--shadow","--fg","--fg-2","--fg-3","--red","--red-hi","--red-ink","--ok","--warn","--ease","--d1","--d2","--d3"])
  assert.match(theme, new RegExp(`${t}\\s*:`), `theme.css must define ${t}`);
assert.match(theme, /html\[data-motion="slow"\][^{]*\{[^}]*--d1:\s*480ms/, "slow --d1 = 480ms, set directly");
assert.match(theme, /html\[data-motion="quick"\][^{]*\{[^}]*--d3:\s*240ms/, "quick --d3 = 240ms, set directly");
assert.match(theme, /prefers-reduced-motion:\s*reduce/, "reduced motion override");
assert.doesNotMatch(theme, /--slow\b|calc\([^)]*--slow/, "no motion multiplier variable");
for (const m of theme.matchAll(/font-weight:\s*(\d+)|font:\s*(\d{3})\s/g)) assert.ok(+(m[1]||m[2]) <= 700, `weight ${m[1]||m[2]} > 700`);
assert.doesNotMatch(theme, /box-shadow:[^;]*(rgba\(224,\s*39,\s*46|var\(--red\))/, "no red glow");
console.log(`check-theme: PASS (${PAGES.length} pages)`);
