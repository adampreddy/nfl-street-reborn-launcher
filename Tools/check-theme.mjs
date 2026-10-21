// Every product page gets the new look the same way, and theme.css holds the rules.
//     node Tools/check-theme.mjs
import { readFileSync, existsSync, readdirSync } from "node:fs";
import assert from "node:assert/strict";
const ui = "crates/reborn-ui/ui/";
const PAGES = ["index","controls","match","leaderboard","mods","music","onboarding","online","profile","recovery","settings","stats","league"];
// Pages moved onto the design system load exactly theme.css + their own page file (spec §11) and say so
// with body.themed. Later tasks add pages here as they move.
const THEMED = { index: "pages/play.css", settings: "pages/settings.css", mods: "pages/mods.css", music: "pages/music.css", controls: "pages/controls.css", stats: "pages/stats.css", profile: "pages/profile.css",
  recovery: "pages/recovery.css", match: "pages/game.css", leaderboard: "pages/leaderboard.css", online: "pages/online.css",
  league: "pages/league.css", onboarding: "onboarding.css",
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
// Spec §11: the old stylesheets are retired once no page loads them. Every shell page is on the design system; the
// developer harness (spec §9, untouched) keeps its own sheet, renamed from styles.css.
const RETIRED = ["styles.css", "launcher.css", "product-ui.css", "finish.css", "shell.css", "play.css", "players.css", "stats.css", "onboarding-polish.css", "live-view.css", "music.css", "profile.css",
  // The old native match view (live.html), retired 10-08: a live game opens the site-styled match page (match.html).
  "live.html", "live.js", "live-game-render.js", "pages/live.css"];
for (const p of PAGES) assert.ok(THEMED[p], `${p} must load only theme.css + its page file (spec §11)`);
for (const f of RETIRED) assert.ok(!existsSync(ui + f), `ui/${f} is retired (spec §11) and must be gone`);
for (const f of readdirSync(ui).filter((x) => x.endsWith(".html"))) {
  const html = readFileSync(ui + f, "utf8");
  for (const r of RETIRED) assert.ok(!html.includes(`href="${r}"`), `${f} still loads ${r}`);
}
assert.match(readFileSync(ui + "harness.html", "utf8"), /href="harness\.css"/, "the developer harness keeps its own stylesheet");
assert.doesNotMatch(theme, /--(bg|panel|line|muted|accent|accent-border|control-bg|control-border|panel-border|selected-bg|focus|teal|motion-ease|motion-fast|motion-normal)\s*:/, "the old stylesheets' variable aliases are gone");
// P4-17: the element rule for buttons has no specificity of its own, so component classes win without page prefixes.
assert.match(theme, /:where\(body\.shell\) button,/, "theme.css's button element rule sits in :where(body.shell)");
assert.doesNotMatch(theme.replace(/\/\*[\s\S]*?\*\//g, ""), /(^|[},])\s*body\.shell button[,{]/m, "no bare body.shell button element rule (0,1,2)");
// DESIGN.md is the written form of theme.css (spec §11): it keeps its opening warning and names every token.
const design = readFileSync(ui + "DESIGN.md", "utf8");
assert.match(design, /If a value is not in this file, it is not a decision anyone gets to make in a stylesheet\./, "DESIGN.md keeps its opening warning");
assert.doesNotMatch(design, /Direction A, approved/, "DESIGN.md no longer describes Direction A as current");
for (const t of ["--mica-tint", "--fallback-bg", "--glass-1", "--glass-2", "--glass-3", "--stroke", "--stroke-hi", "--sheen", "--shadow", "--fg", "--fg-2", "--fg-3", "--red", "--red-hi", "--red-ink", "--ok", "--warn", "--ease", "--d1", "--d2", "--d3"])
  assert.ok(design.includes(`\`${t}\``), `DESIGN.md names ${t}`);
// P4-18: every token theme.css defines on :root is named (a token removed from theme.css must leave DESIGN.md too).
const rootBlock = theme.slice(theme.indexOf(":root{"), theme.indexOf("\n}", theme.indexOf(":root{")));
for (const m of new Set([...rootBlock.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((x) => x[1]))) {
  if (/^--av-[0-7]$/.test(m)) continue; // named as a range
  assert.ok(design.includes(m), `DESIGN.md names ${m}`);
}
console.log(`check-theme: PASS (${PAGES.length} pages)`);
