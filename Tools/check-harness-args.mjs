// Check that every harness row sends the arguments its Rust command declares.
//
// Why this exists. A Tauri command's arguments are matched by NAME at runtime,
// through serde. A row that sends `obPort` to a command expecting `port` gets a
// deserialization error when the button is clicked, and nothing before that
// click says so: the JavaScript parses, the Rust compiles, and the two are only
// introduced to each other by a string.
//
// It found two real faults on the day it was written. Nine of the new first run
// rows used prefixed input keys that did not match the parameter names, so every
// one of them would have sent null. And three older rows, mods_plan, mods_apply
// and mods_verify, sent `packages` as an array of plain strings when PackageRef
// is a struct of id and version, so those three had never been able to run.
//
// The expected names are read out of main.rs rather than listed here, so this
// cannot drift from the commands it is checking.
//
//     node Tools/check-harness-args.mjs

import { readFileSync, readdirSync } from "fs";

// Every source file of the window crate: commands live in main.rs and in its
// modules (content_updates.rs, music_page.rs).
const UI_DIR = "crates/reborn-ui/src";
const APP = "crates/reborn-ui/ui/app.js";

// ---------------------------------------------------------------------------
// What Rust declares
// ---------------------------------------------------------------------------

/// snake_case to the camelCase Tauri converts from.
function camel(s) {
  return s.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
}

function rustCommands(src) {
  const out = new Map();
  // #[tauri::command] then an fn, whose parameter list runs to the closing
  // paren before the return arrow. Parameters may be spread over many lines.
  const re = /#\[tauri::command\]\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\(([\s\S]*?)\)\s*->/g;
  let m;
  while ((m = re.exec(src))) {
    const [, name, params] = m;
    const args = [];
    // Split on top level commas only: a type like Option<Vec<T>> has none, but
    // a tuple or a nested generic with two parameters would.
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

    for (const raw of args) {
      const arg = raw.replace(/\/\/.*$/gm, "").trim();
      if (!arg) continue;
      const named = arg.match(/^(\w+)\s*:\s*([\s\S]+)$/);
      if (!named) continue;
      const id = named[1];
      const type = named[2].replace(/\s+/g, " ").trim();
      // Tauri injects these rather than taking them from the caller.
      if (["app", "window", "state", "webview"].includes(id)) continue;
      out.set(name, [...(out.get(name) || []), { name: camel(id), type }]);
    }
    if (!out.has(name)) out.set(name, []);
  }
  return out;
}

// ---------------------------------------------------------------------------
// What the harness sends
// ---------------------------------------------------------------------------

function harness(src) {
  const fake = () =>
    new Proxy(
      { style: {}, classList: { add() {}, remove() {} }, value: "", textContent: "" },
      {
        get: (t, k) => (k in t ? t[k] : () => fake()),
        set: (t, k, v) => ((t[k] = v), true),
      },
    );
  global.localStorage = { getItem: () => null, setItem() {}, removeItem() {} };
  global.document = { getElementById: () => fake(), createElement: () => fake() };
  global.window = {};

  const patched =
    src.replace(/^const bridge\b.*$/m, "const bridge = null;") +
    "\nexport { FEATURES, buildArgs };";
  return import("data:text/javascript;base64," + Buffer.from(patched).toString("base64"));
}

// ---------------------------------------------------------------------------

const declared = new Map();
for (const name of readdirSync(UI_DIR).filter((n) => n.endsWith(".rs")).sort()) {
  for (const [cmd, args] of rustCommands(readFileSync(`${UI_DIR}/${name}`, "utf8"))) declared.set(cmd, args);
}
const { FEATURES, buildArgs } = await harness(readFileSync(APP, "utf8"));
const rows = FEATURES.flatMap((g) => g.items);

console.log(`${declared.size} commands in Rust, ${rows.length} rows in the harness\n`);

const problems = [];

for (const f of rows) {
  const want = declared.get(f.cmd);
  if (!want) {
    problems.push(`${f.cmd}: the harness has a row for a command Rust does not declare`);
    continue;
  }

  // Fill every box, the way a player would before clicking.
  const fields = {};
  for (const inp of f.inputs || []) {
    const value = inp.number
      ? "1"
      : inp.refs
        ? "some-mod@1.0.0"
        : inp.bool
          ? "true"
          : "value";
    fields[inp.key] = {
      box: { value },
      list: inp.list,
      refs: inp.refs,
      number: inp.number,
      bool: inp.bool,
    };
  }

  let sent;
  try {
    sent = buildArgs(f, fields, { checked: true });
  } catch (e) {
    problems.push(`${f.cmd}: building its arguments threw: ${e.message}`);
    continue;
  }

  const sentKeys = Object.keys(sent);
  const wantNames = want.map((a) => a.name);
  const missing = wantNames.filter((k) => !sentKeys.includes(k));
  const extra = sentKeys.filter((k) => !wantNames.includes(k));

  // A missing argument is always a fault: serde needs the key even to read it
  // as None, unless the row simply never sends it and the field is Option, in
  // which case Tauri still requires the key to be present as null.
  if (missing.length) {
    problems.push(`${f.cmd}: does not send ${missing.join(", ")}`);
  }
  if (extra.length) {
    problems.push(
      `${f.cmd}: sends ${extra.join(", ")}, which the command does not take`,
    );
  }

  // Now the types. This is the half that matters, because a name can match
  // while the value is the wrong shape, and serde rejects that at click time.
  for (const arg of want) {
    const value = sent[arg.name];
    if (value === undefined) continue;
    const inner = arg.type.replace(/^Option<(.*)>$/, "$1").trim();

    // A Vec of anything other than String needs objects. A row that sends
    // strings compiles, parses, and fails on the first click. This is exactly
    // the fault mods_plan, mods_apply and mods_verify had.
    const vec = inner.match(/^Vec<(.+)>$/);
    if (vec) {
      const of = vec[1].trim();
      const wantsObjects = of !== "String" && /^[A-Z]/.test(of);
      const sendsStrings =
        Array.isArray(value) && value.length > 0 && typeof value[0] === "string";
      if (wantsObjects && sendsStrings) {
        problems.push(
          `${f.cmd}: sends ${arg.name} as strings, but it is Vec<${of}>. ` +
            `Set refs: true on that input so each entry becomes an object.`,
        );
      }
      if (wantsObjects && !Array.isArray(value)) {
        problems.push(
          `${f.cmd}: sends ${arg.name} as ${typeof value}, but it is Vec<${of}>.`,
        );
      }
      continue;
    }

    // A number cannot arrive as a string. A text box gives one unless the
    // input says number: true.
    if (/^(u8|u16|u32|u64|usize|i8|i16|i32|i64|isize|f32|f64)$/.test(inner)) {
      if (typeof value !== "number" && value !== null) {
        problems.push(
          `${f.cmd}: sends ${arg.name} as ${typeof value}, but it is ${inner}. ` +
            `Set number: true on that input.`,
        );
      }
      continue;
    }

    if (inner === "bool" && typeof value !== "boolean") {
      problems.push(
        `${f.cmd}: sends ${arg.name} as ${typeof value}, but it is bool.`,
      );
    }
  }
}

// A command with no row cannot be exercised at all. Reported as a note rather
// than a failure: some commands are called by other commands, not by a button.
const rowed = new Set(rows.map((f) => f.cmd));
const unrowed = [...declared.keys()].filter((c) => !rowed.has(c));
if (unrowed.length) {
  console.log(`No harness row, so unreachable from the window:\n  ${unrowed.join("\n  ")}\n`);
}

if (problems.length) {
  console.log("FAIL");
  for (const p of problems) console.log("  " + p);
  process.exit(1);
}
console.log(`PASS: all ${rows.length} rows send exactly what their command declares`);
