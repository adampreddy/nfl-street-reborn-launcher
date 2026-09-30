// Put a page on the design system: its <head> loads exactly theme.css + its own stylesheet, and <body> says so with the
// `themed` class (Tools/check-theme.mjs). Nothing else in the page changes: the DOM stays identical, so
// Tools/compare-page-layout.mjs can compare it element by element.
//     node Tools/theme-page.mjs <page> <stylesheet>        e.g.  node Tools/theme-page.mjs recovery pages/recovery.css
import { readFileSync, writeFileSync } from 'node:fs';

const [page, sheet] = process.argv.slice(2);
if (!page || !sheet) { console.error('usage: node Tools/theme-page.mjs <page> <stylesheet>'); process.exit(2); }
const file = `crates/reborn-ui/ui/${page}.html`;
const html = readFileSync(file, 'utf8');
const nl = html.includes('\r\n') ? '\r\n' : '\n';
const end = html.indexOf('</head>');
let head = html.slice(0, end);
const lines = [...head.matchAll(/[ \t]*(?:<link rel="stylesheet" href="[^"]+">)+[ \t]*\r?\n/g)];
if (!lines.length) throw new Error(`${page}: no stylesheet lines in <head>`);
lines.forEach((m, i) => { head = head.replace(m[0], i === 0 ? `  <link rel="stylesheet" href="theme.css"><link rel="stylesheet" href="${sheet}">${nl}` : ''); });
let out = head + html.slice(end);
out = out.replace(/<body class="([^"]*)"/, (m, cls) => (/\bthemed\b/.test(cls) ? m : `<body class="${cls.replace(/\bshell\b/, 'shell themed')}"`));
writeFileSync(file, out);
console.log(`${page}: theme.css + ${sheet}, body.themed`);
