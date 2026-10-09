// Check that every PAGE that invokes a Tauri command sends the arguments that
// command declares.
//
// check-harness-args.mjs already does this, but only for the Advanced page's
// row table. Every other page calls commands directly, and those calls were
// checked by nobody. That gap shipped a real fault: home.js's Play button
// called `launch` without `modsDisabled`, so pressing Play failed with
//
//     invalid args `modsDisabled` for command `launch`:
//     command launch missing required key modsDisabled
//
// while app.js and recovery.js both passed the key and the harness checker
// reported all 65 rows correct. The one button a player actually presses was
// the one nothing covered.
//
// A Tauri command's arguments are matched by NAME at runtime through serde, so
// a missing or misspelled key is invisible until the click. The expected names
// are read out of main.rs so this cannot drift from what it checks.
//
//     node Tools/check-page-call-args.mjs

import { readFileSync, readdirSync } from "fs";
import { join } from "path";

// main.rs and the command modules whose commands the pages call by name (the League page calls league_edition's,
// the Online page online_mm's); each file's #[tauri::command] fns are read the same way.
const RUST = [
  "crates/reborn-ui/src/main.rs",
  "crates/reborn-ui/src/league_edition.rs",
  "crates/reborn-ui/src/online_mm.rs",
];
const UI_DIR = "crates/reborn-ui/ui";
// app.js is the Advanced harness, covered in full by check-harness-args.mjs.
const SKIP = new Set(["app.js"]);

/// snake_case to the camelCase Tauri converts from.
const camel = (s) => s.replace(/_([a-z])/g, (_, c) => c.toUpperCase());

// ---------------------------------------------------------------------------
// What Rust declares, and which of it is mandatory
// ---------------------------------------------------------------------------
function rustCommands(src) {
  const out = new Map();
  // The parameter list ends at the ")" before the return type or, for a command that returns nothing
  // (presence_page), before the body: requiring "->" let such a command run on into the next one's parameters.
  const re = /#\[tauri::command\]\s*(?:pub(?:\(crate\))?\s+)?(?:async\s+)?fn\s+(\w+)\s*\(([\s\S]*?)\)\s*(?:->|\{)/g;
  let m;
  while ((m = re.exec(src))) {
    const [, name, params] = m;
    const args = [];
    let depth = 0;
    let current = "";
    for (const ch of params) {
      if ("<([".includes(ch)) depth++;
      else if (">)]".includes(ch)) depth--;
      if (ch === "," && depth === 0) {
        args.push(current);
        current = "";
      } else current += ch;
    }
    if (current.trim()) args.push(current);

    const fields = [];
    for (const raw of args) {
      const arg = raw.replace(/\/\/.*$/gm, "").trim();
      if (!arg) continue;
      const named = arg.match(/^(\w+)\s*:\s*([\s\S]+)$/);
      if (!named) continue;
      const id = named[1];
      const type = named[2].replace(/\s+/g, " ").trim();
      if (["app", "window", "state", "webview"].includes(id)) continue;
      // Option<T> deserializes from a missing key. Everything else must be
      // sent, which is exactly what the modsDisabled failure was.
      fields.push({ name: camel(id), required: !/^Option\s*</.test(type) });
    }
    out.set(name, fields);
  }
  return out;
}

// ---------------------------------------------------------------------------
// Reading a balanced expression out of the source
// ---------------------------------------------------------------------------
function balanced(src, start) {
  const open = src[start];
  const close = { "{": "}", "(": ")", "[": "]" }[open];
  if (!close) return null;
  let depth = 0;
  let inStr = null;
  for (let i = start; i < src.length; i++) {
    const ch = src[i];
    if (inStr) {
      if (ch === "\\") i++;
      else if (ch === inStr) inStr = null;
      continue;
    }
    // Comments first. One holding a lone quote would otherwise open a string
    // that never closes and swallow the rest of the file.
    if (ch === "/" && src[i + 1] === "/") {
      i = src.indexOf("\n", i);
      if (i === -1) return null;
      continue;
    }
    if (ch === "/" && src[i + 1] === "*") {
      i = src.indexOf("*/", i + 2);
      if (i === -1) return null;
      i++;
      continue;
    }
    if (ch === '"' || ch === "'" || ch === "`") inStr = ch;
    else if (ch === open) depth++;
    else if (ch === close) {
      depth--;
      if (depth === 0) return { text: src.slice(start, i + 1), end: i };
    }
  }
  return null;
}

/// Top level keys of an object literal, plus whether it spreads something.
function literalKeys(text) {
  const inner = text.slice(1, -1);
  const keys = [];
  let spread = false;
  let depth = 0;
  let inStr = null;
  let token = "";
  const flush = () => {
    const t = token.trim();
    token = "";
    if (!t) return;
    if (t.startsWith("...")) { spread = true; return; }
    const k = t.match(/^([A-Za-z_$][\w$]*)\s*:/) || t.match(/^([A-Za-z_$][\w$]*)\s*$/);
    if (k) keys.push(k[1]);
  };
  for (let i = 0; i < inner.length; i++) {
    const ch = inner[i];
    if (inStr) {
      if (ch === "\\") { token += ch + inner[++i]; continue; }
      if (ch === inStr) inStr = null;
      token += ch;
      continue;
    }
    // A comment above a key is not part of the key. Without this, the comment
    // explaining why modsDisabled is required made the checker declare
    // modsDisabled missing.
    if (ch === "/" && inner[i + 1] === "/") {
      const nl = inner.indexOf("\n", i);
      if (nl === -1) break;
      i = nl;
      continue;
    }
    if (ch === "/" && inner[i + 1] === "*") {
      const close = inner.indexOf("*/", i + 2);
      if (close === -1) break;
      i = close + 1;
      continue;
    }
    if (ch === '"' || ch === "'" || ch === "`") { inStr = ch; token += ch; continue; }
    if ("{([".includes(ch)) depth++;
    if ("})]".includes(ch)) depth--;
    if (ch === "," && depth === 0) { flush(); continue; }
    token += ch;
  }
  flush();
  return { keys, spread };
}

/// A helper like launchArgs() that returns an object literal.
function helperReturns(src, name) {
  const re = new RegExp(`function\\s+${name}\\s*\\([^)]*\\)\\s*\\{`);
  const m = re.exec(src);
  if (!m) return null;
  const body = balanced(src, m.index + m[0].length - 1);
  if (!body) return null;
  const ret = body.text.indexOf("return {");
  if (ret === -1) return null;
  const obj = balanced(body.text, body.text.indexOf("{", ret));
  return obj ? literalKeys(obj.text) : null;
}

// ---------------------------------------------------------------------------
const declared = new Map(RUST.flatMap((f) => [...rustCommands(readFileSync(f, "utf8"))]));
const files = readdirSync(UI_DIR).filter((f) => f.endsWith(".js") && !SKIP.has(f));

const problems = [];
let checked = 0;
let indirect = 0;

for (const file of files) {
  const src = readFileSync(join(UI_DIR, file), "utf8");

  for (const [cmd, fields] of declared) {
    // Every place this command's name appears as a string followed by an
    // argument. Catches call("launch", {...}) and act("X", "launch", args, ...)
    // alike, without depending on which wrapper a page happens to use.
    const re = new RegExp(`["']${cmd}["']\\s*,\\s*`, "g");
    let m;
    while ((m = re.exec(src))) {
      const at = m.index + m[0].length;
      const line = src.slice(0, m.index).split("\n").length;
      const where = `${file}:${line} ${cmd}`;

      let got = null;
      if (src[at] === "{") {
        const obj = balanced(src, at);
        if (obj) got = literalKeys(obj.text);
      } else {
        // A helper call such as launchArgs({ ... }): take the helper's own
        // literal and add whatever this call site overrides.
        const call = src.slice(at).match(/^([A-Za-z_$][\w$]*)\s*\(/);
        if (call) {
          const base = helperReturns(src, call[1]);
          if (base) {
            got = { keys: [...base.keys], spread: base.spread };
            const argAt = at + call[0].length - 1;
            const argObj = balanced(src, argAt);
            if (argObj && argObj.text.startsWith("(")) {
              const first = argObj.text.indexOf("{");
              if (first !== -1) {
                const lit = balanced(argObj.text, first);
                if (lit) got.keys.push(...literalKeys(lit.text).keys);
              }
            }
          }
        }
      }

      if (!got) {
        indirect++;
        console.log(`  ? ${where}: arguments are not a literal, not checked`);
        continue;
      }
      checked++;
      const missing = fields.filter((f) => f.required && !got.keys.includes(f.name));
      if (missing.length && !got.spread) {
        problems.push(
          `${where}: missing required ${missing.map((f) => f.name).join(", ")}` +
            ` (sends ${got.keys.join(", ") || "nothing"})`,
        );
      }
      const unknown = got.keys.filter((k) => !fields.some((f) => f.name === k));
      if (unknown.length) {
        problems.push(`${where}: sends ${unknown.join(", ")}, which the command does not declare`);
      }
    }
  }
}

console.log(
  `\n${declared.size} commands in Rust, ${checked} call site(s) checked across ${files.length} page(s)` +
    (indirect ? `, ${indirect} not literal` : ""),
);

if (problems.length) {
  console.error("\nFAIL");
  for (const p of problems) console.error("  " + p);
  process.exit(1);
}
console.log("PASS: every page sends what its command declares");
