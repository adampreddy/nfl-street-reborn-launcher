// Page stylesheets hold layout only (spec §11, §13): no colour literals, no weight above 700.
// Every colour comes from a theme.css token.
//     node Tools/check-theme-tokens.mjs [dir]      (default crates/reborn-ui/ui/pages)
import { readFileSync, readdirSync, existsSync } from "node:fs";
import path from "node:path";

const dir = path.resolve(process.argv[2] || "crates/reborn-ui/ui/pages");
if (!existsSync(dir)) { console.log(`check-theme-tokens: PASS (no ${dir} yet)`); process.exit(0); }
const problems = [];
let files = 0;
for (const name of readdirSync(dir).filter((f) => f.endsWith(".css"))) {
  files++;
  // Comments out, newlines kept so line numbers stay true.
  const css = readFileSync(path.join(dir, name), "utf8").replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "));
  // Innermost { … } blocks are declarations; selectors (#main, #play-now) sit outside them.
  for (const block of css.matchAll(/\{([^{}]*)\}/g)) {
    const line = css.slice(0, block.index).split("\n").length;
    const body = block[1];
    const where = `${name}:${line}`;
    for (const m of body.matchAll(/#[0-9a-fA-F]{3,8}\b/g)) problems.push(`${where}: colour literal ${m[0]}`);
    for (const m of body.matchAll(/\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\s*\(/gi)) problems.push(`${where}: colour function ${m[0]}`);
    for (const m of body.matchAll(/font-weight\s*:\s*(\d+)/g)) if (+m[1] > 700) problems.push(`${where}: font-weight ${m[1]}`);
    for (const m of body.matchAll(/(?:^|[;\s])font\s*:\s*(\d{3})\b/g)) if (+m[1] > 700) problems.push(`${where}: font weight ${m[1]}`);
  }
}
if (problems.length) { console.error("check-theme-tokens: FAIL\n  " + problems.join("\n  ")); process.exit(1); }
console.log(`check-theme-tokens: PASS (${files} page stylesheet(s))`);
