// The launcher's game page (crates/reborn-ui/ui/game.html) is the site's /games/<id> page: the same renderers, the
// same stylesheet, the same team colours. They are VENDORED from the Reborn Backend repo into crates/reborn-ui/ui/site/,
// never ported or edited by hand. This script is the only thing that writes them:
//
//   live-render.js, site-model.js, live-card-core.js   copied unchanged under a 2-line header naming the commit
//   live.css                                            the site's live.css, every rule scoped under #site-game (see
//                                                       scopeCss), its viewport breakpoints turned into queries on
//                                                       the page's own width (pageQuery), fonts pointed at the
//                                                       launcher's own ../fonts/
//   teams.css                                           the .tg-<tgid> colour rules, built from assets/live/teams.json
//                                                       exactly as the backend's src/liveAssets.js builds them, scoped
//
//     node Tools/sync-site-render.mjs [--backend <path>]   re-copy from the backend (default: ../Reborn Backend)
//     node Tools/sync-site-render.mjs --check              fail when ui/site/ differs from what a sync would write
//                                                          (skipped, with a note, when the backend is not on disk)
//
// live-card-core.js is also the Live bot's file (a byte copy): --check and the sync both insist the vendored copy is
// byte-identical to the backend's apart from the header.
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';

const root = path.resolve(import.meta.dirname, '..');
const out = path.join(root, 'crates/reborn-ui/ui/site');
const args = process.argv.slice(2);
const check = args.includes('--check');
const at = args.indexOf('--backend');
const backend = path.resolve(at >= 0 ? args[at + 1] : process.env.REBORN_BACKEND || path.join(root, '..', 'Reborn Backend'));
const web = path.join(backend, 'src/web');
const JS = ['live-render.js', 'site-model.js', 'live-card-core.js'];
export const SCOPE = '#site-game';
// The site's <body class="site"> becomes this element inside #site-game: a tag, like body, so every scoped selector
// gains exactly one id over the site's own and the site's cascade order between its rules is unchanged.
export const BODY = 'nsr-body';
// The site's font files, by name, and the launcher's copies (font-src 'self').
const FONTS = { 'InterVariable.woff2': '../fonts/InterVariable.woff2', 'Anton-Regular.ttf': '../fonts/Anton.woff2' };

const lf = (s) => s.replace(/\r\n/g, '\n');
const header = (name, commit, comment) => {
  const from = name === 'teams.css' ? 'assets/live/teams.json' : `src/web/${name}`;
  const lines = [`Vendored from Reborn Backend ${from} @${commit}, do not edit: run Tools/sync-site-render.mjs.`,
    name === 'teams.css' ? `The .tg-<tgid> rules src/liveAssets.js serves as /live-assets/teams.css, scoped under ${SCOPE}.`
      : name.endsWith('.css') ? `Scoped under ${SCOPE} (the site's body is <${BODY} class="site">), breakpoints on the page width, the launcher's fonts.`
        : 'The launcher game page (game.js) draws with it exactly as the site does.'];
  return comment === 'css' ? lines.map((l) => `/* ${l} */\n`).join('') : lines.map((l) => `// ${l}\n`).join('');
};

/** Split a selector list on its top-level commas (never inside :is(...) or [attr="a,b"]). */
function splitList(s) {
  const parts = [];
  let depth = 0, quote = null, cur = '';
  for (const ch of s) {
    if (quote) { if (ch === quote) quote = null; cur += ch; continue; }
    if (ch === '"' || ch === "'") quote = ch;
    else if (ch === '(' || ch === '[') depth++;
    else if (ch === ')' || ch === ']') depth--;
    else if (ch === ',' && depth === 0) { parts.push(cur.trim()); cur = ''; continue; }
    cur += ch;
  }
  if (cur.trim()) parts.push(cur.trim());
  return parts;
}

/** One selector of the site's under the launcher's scope: :root and html become the scope itself, body (and
 *  body.site, body.missing, ...) the scope's <nsr-body>, anything else a descendant of the scope. */
export function scopeSelector(sel) {
  const s = sel.trim().replace(/\s+/g, ' ');
  if (s === ':root' || s === 'html') return SCOPE;
  if (/^html[\s.:#[]/.test(s)) throw new Error(`unexpected html selector: ${s}`);
  if (/^body(?![\w-])/.test(s)) return `${SCOPE} ${BODY}${s.slice(4)}`;
  return `${SCOPE} ${s}`;
}

// The site's breakpoints are viewport widths; in the launcher the page sits beside the sidebar, so the same viewport
// is a narrower page. Each `@media (max-width: N)` becomes a query on the page's own width instead: #site-game is the
// size container `site-page` (pages/game.css), as wide as the site's .pg card, and the site's .pg is the viewport less
// its body padding (16 px a side at 600 px and under, 22 px above). So the launcher switches layout at the page width
// the site does, whatever width the sidebar takes.
export const CONTAINER = 'site-page';
function pageQuery(prelude) {
  const m = /^@media \(max-width: (\d+)px\)$/.exec(prelude);
  if (!m) throw new Error(`live.css media query the scoper does not know: ${prelude}`);
  const n = Number(m[1]);
  return `@container ${CONTAINER} (max-width: ${n - (n <= 600 ? 32 : 44)}px)`;
}

/** The matching close brace of the block opened at css[open]. */
function blockEnd(css, open) {
  let depth = 0, quote = null;
  for (let i = open; i < css.length; i++) {
    const ch = css[i];
    if (quote) { if (ch === quote && css[i - 1] !== '\\') quote = null; continue; }
    if (ch === '"' || ch === "'") quote = ch;
    else if (ch === '{') depth++;
    else if (ch === '}' && --depth === 0) return i;
  }
  throw new Error('unbalanced braces in live.css');
}

/** The site's live.css, comments dropped, every style rule scoped (inside @media / @container / @supports too),
 *  @font-face sources pointed at the launcher's fonts, @keyframes left as they are. */
export function scopeCss(source) {
  const css = lf(source).replace(/\/\*[\s\S]*?\*\//g, '');
  const walk = (text, indent) => {
    let i = 0, outText = '';
    while (i < text.length) {
      const open = text.indexOf('{', i);
      if (open < 0) { if (text.slice(i).trim()) throw new Error(`stray text in live.css: ${text.slice(i).trim().slice(0, 60)}`); break; }
      const prelude = text.slice(i, open).trim().replace(/\s+/g, ' ');
      const close = blockEnd(text, open);
      const body = text.slice(open + 1, close);
      if (/^@media\b/.test(prelude)) {
        outText += `${indent}${pageQuery(prelude)} {\n${walk(body, indent + '  ')}${indent}}\n`;
      } else if (/^@(container|supports)\b/.test(prelude)) {
        outText += `${indent}${prelude} {\n${walk(body, indent + '  ')}${indent}}\n`;
      } else if (/^@font-face\b/.test(prelude)) {
        const fixed = body.replace(/url\("?([^")]+)"?\)\s*format\("[^"]+"\)/g, (_, url) => {
          const to = FONTS[path.posix.basename(url)];
          if (!to) throw new Error(`live.css names a font the launcher does not ship: ${url}`);
          return `url("${to}") format("woff2")`;
        });
        outText += `${indent}${prelude} { ${fixed.trim().replace(/\s+/g, ' ')} }\n`;
      } else if (/^@keyframes\b/.test(prelude)) {
        outText += `${indent}${prelude} {${body}}\n`;
      } else if (prelude.startsWith('@')) {
        throw new Error(`live.css at-rule the scoper does not know: ${prelude}`);
      } else {
        if (/url\(/.test(body)) throw new Error(`live.css rule loads a url the launcher cannot serve: ${prelude}`);
        outText += `${indent}${splitList(prelude).map(scopeSelector).join(', ')} { ${body.trim().replace(/\s+/g, ' ')} }\n`;
      }
      i = close + 1;
    }
    return outText;
  };
  return walk(css, '');
}

// ---- teams.css: src/liveAssets.js's teamsCss rule shape (inkFor: black or white, whichever contrasts more) ----
const HEX = /^#[0-9A-F]{6}$/;
const channel = (v) => { const x = v / 255; return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4; };
const luminance = (hex) => { const [r, g, b] = [1, 3, 5].map((i) => channel(parseInt(hex.slice(i, i + 2), 16))); return 0.2126 * r + 0.7152 * g + 0.0722 * b; };
const contrast = (a, b) => { const [x, y] = [luminance(a), luminance(b)].sort((m, n) => n - m); return (x + 0.05) / (y + 0.05); };
const inkFor = (hex) => (contrast('#000000', hex) >= contrast('#FFFFFF', hex) ? '#000000' : '#FFFFFF');
export function teamsCss(teamsJson) {
  const rows = [];
  for (const [key, t] of Object.entries(teamsJson)) {
    const tgid = Number(key);
    if (!Number.isSafeInteger(tgid) || tgid < 0 || String(tgid) !== key || !t || !HEX.test(t.primary) || !HEX.test(t.secondary)) continue;
    rows.push({ tgid, primary: t.primary, secondary: t.secondary });
  }
  return rows.sort((a, b) => a.tgid - b.tgid).map((t) =>
    `${SCOPE} .tg-${t.tgid}{--team:${t.primary};--team-ink:${inkFor(t.primary)};--team2:${t.secondary};--team2-ink:${inkFor(t.secondary)}}`).join('\n') + '\n';
}

/** Every file a sync writes, path -> text. */
function build(commit) {
  const files = {};
  for (const name of JS) files[name] = header(name, commit, 'js') + lf(readFileSync(path.join(web, name), 'utf8'));
  files['live.css'] = header('live.css', commit, 'css') + scopeCss(readFileSync(path.join(web, 'live.css'), 'utf8'));
  files['teams.css'] = header('teams.css', commit, 'css') + teamsCss(JSON.parse(readFileSync(path.join(backend, 'assets/live/teams.json'), 'utf8')));
  return files;
}

/** The vendored text without its 2-line header. */
const body = (text) => text.split('\n').slice(2).join('\n');

function main() {
  if (!existsSync(web)) {
    if (check) { console.log(`sync-site-render: SKIP (no backend at ${backend}; pass --backend <path>)`); return; }
    console.error(`sync-site-render: no backend at ${backend} (pass --backend <path>)`); process.exit(1);
  }
  let commit = 'unknown';
  try { commit = execFileSync('git', ['-C', backend, 'rev-parse', '--short', 'HEAD'], { encoding: 'utf8' }).trim(); } catch { /* not a checkout */ }
  let dirty = '';
  try { dirty = execFileSync('git', ['-C', backend, 'status', '--porcelain', '--', 'src/web', 'assets/live/teams.json'], { encoding: 'utf8' }).trim(); } catch { /* not a checkout */ }
  if (dirty) commit += '+local';
  const files = build(commit);
  const core = lf(readFileSync(path.join(web, 'live-card-core.js'), 'utf8'));
  if (body(files['live-card-core.js']) !== core) throw new Error('live-card-core.js would not be byte-identical to the backend copy');
  if (check) {
    const problems = [];
    for (const [name, text] of Object.entries(files)) {
      const p = path.join(out, name);
      if (!existsSync(p)) { problems.push(`${name}: missing`); continue; }
      const have = readFileSync(p, 'utf8');
      if (have.includes('\r')) problems.push(`${name}: CRLF line endings`);
      // The header names the commit it came from; compare the content under it.
      if (body(have) !== body(text)) problems.push(`${name}: differs from the backend (run node Tools/sync-site-render.mjs)`);
    }
    if (problems.length) { console.error('sync-site-render --check: FAIL\n  ' + problems.join('\n  ')); process.exit(1); }
    console.log(`sync-site-render --check: PASS (${Object.keys(files).length} files match ${backend} @${commit})`);
    return;
  }
  mkdirSync(out, { recursive: true });
  for (const [name, text] of Object.entries(files)) writeFileSync(path.join(out, name), text);
  console.log(`sync-site-render: wrote ${Object.keys(files).join(', ')} from ${backend} @${commit}`);
}

if (import.meta.url === `file:///${process.argv[1].replace(/\\/g, '/')}` || process.argv[1]?.endsWith('sync-site-render.mjs')) main();
