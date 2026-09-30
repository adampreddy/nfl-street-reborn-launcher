// Failure MESSAGES of the checkers recorded in target/p4-before (plan 4 baseline), normalised so they can be compared
// across tasks: a message is keyed on its text, never on file:line (later tasks edit those checkers and shift lines).
//     node Tools/baseline-messages.mjs [dir]            writes <dir>/messages.txt and prints it
//     node Tools/baseline-messages.mjs [dir] --compare <otherDir>   prints checkers whose message set differs
import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const args = process.argv.slice(2);
const dir = path.resolve(args[0] && !args[0].startsWith('--') ? args[0] : 'target/p4-before');
const other = args.includes('--compare') ? path.resolve(args[args.indexOf('--compare') + 1]) : null;

export function normalise(line) {
  return line.replace(/\x1b\[[0-9;]*m/g, '').replace(/\[\d+m/g, '')
    .replace(/file:\/\/\/\S+?\.mjs:\d+:\d+/g, '<file>').replace(/[A-Za-z]:\\[^\s:]+\.mjs:\d+:\d+/g, '<file>')
    .replace(/from "data:[^"]*"/g, 'from "data:…"').replace(/\s+/g, ' ').trim();
}
export function messages(log) {
  const out = [];
  for (const raw of log.split(/\r?\n/)) {
    const l = normalise(raw);
    if (!l || /^at /.test(l) || /^(code|name|actual|expected|operator|generatedMessage|diff|log):/.test(l) || /^[\^{}\[\]]+$/.test(l)) continue;
    if (/^FAIL\b/.test(l) || /Error\b.*:/.test(l) || /^missing text/.test(l) || /waiting for/.test(l) || /^[+-] ?'/.test(l) || /^(?:\d+ )?!== \d+$/.test(l)) out.push(l);
  }
  return [...new Set(out)];
}
export function collect(d) {
  const failing = readFileSync(path.join(d, 'checks.txt'), 'utf8').split(/\r?\n/).map((x) => x.split(' ')).filter(([n, c]) => n && /^\d+$/.test(c) && c !== '0');
  return Object.fromEntries(failing.map(([n]) => [n, messages(readFileSync(path.join(d, `${n}.log`), 'utf8'))]));
}
if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const mine = collect(dir);
  const text = Object.entries(mine).map(([n, m]) => `${n}\n${m.map((x) => `  ${x}`).join('\n')}`).join('\n') + '\n';
  writeFileSync(path.join(dir, 'messages.txt'), text);
  if (!other) console.log(text);
  else {
    const theirs = collect(other);
    let diff = 0;
    for (const n of new Set([...Object.keys(mine), ...Object.keys(theirs)])) {
      const a = (mine[n] ?? []).join('\n'), b = (theirs[n] ?? []).join('\n');
      if (a !== b) { diff++; console.log(`DIFFERS ${n}\n  baseline: ${a || '(passed)'}\n  now:      ${b || '(passed)'}`); }
    }
    console.log(diff ? `${diff} checker(s) differ` : 'same failing checkers with the same messages');
    process.exit(diff ? 1 : 0);
  }
}
