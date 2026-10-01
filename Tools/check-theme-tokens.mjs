// Page stylesheets hold layout only (spec §11, §13): no colour literals, no weight above 700. Every colour comes from a
// theme.css token. Checks every .css in a folder, or a single file.
//     node Tools/check-theme-tokens.mjs [path ...]      (default: crates/reborn-ui/ui/pages and crates/reborn-ui/ui/onboarding.css)
import { readFileSync, readdirSync, existsSync, statSync } from "node:fs";
import path from "node:path";

const targets = process.argv.length > 2 ? process.argv.slice(2) : ["crates/reborn-ui/ui/pages", "crates/reborn-ui/ui/onboarding.css"];
const problems = [];
let files = 0;
for (const target of targets.map((t) => path.resolve(t))) {
  if (!existsSync(target)) continue;
  const list = statSync(target).isDirectory() ? readdirSync(target).filter((f) => f.endsWith(".css")).map((f) => path.join(target, f)) : [target];
  for (const file of list) {
    files++;
    const name = path.relative(process.cwd(), file);
    // Comments out, newlines kept so line numbers stay true.
    const css = readFileSync(file, "utf8").replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "));
    // Innermost { … } blocks are declarations; selectors (#main, #play-now) sit outside them.
    for (const block of css.matchAll(/\{([^{}]*)\}/g)) {
      const where = `${name}:${css.slice(0, block.index).split("\n").length}`;
      const body = block[1];
      for (const m of body.matchAll(/#[0-9a-fA-F]{3,8}\b/g)) problems.push(`${where}: colour literal ${m[0]}`);
      for (const m of body.matchAll(/\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch)\s*\(/gi)) problems.push(`${where}: colour function ${m[0]}`);
      for (const m of body.matchAll(/font-weight\s*:\s*(\d+)/g)) if (+m[1] > 700) problems.push(`${where}: font-weight ${m[1]}`);
      for (const m of body.matchAll(/(?:^|[;\s])font\s*:\s*(\d{3})\b/g)) if (+m[1] > 700) problems.push(`${where}: font weight ${m[1]}`);
    }
  }
}
if (problems.length) { console.error("check-theme-tokens: FAIL\n  " + problems.join("\n  ")); process.exit(1); }
console.log(`check-theme-tokens: PASS (${files} stylesheet(s))`);
