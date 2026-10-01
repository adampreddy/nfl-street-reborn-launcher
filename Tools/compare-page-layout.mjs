// Layout snapshots of a page before and after its stylesheets change (spec §9 "no layout work", §13).
//   node Tools/compare-page-layout.mjs capture <page> <dir>
//        writes <dir>/<page>.json and, per state and window size, <state>-<w>.png (top) and <state>-<w>-end.png (scrolled)
//   node Tools/compare-page-layout.mjs diff <page> <beforeDir> <afterDir> [--loose <regex>]... [--loose-prop <regex> <property>]...
// Each element's box is taken relative to its parent's box, so a taller heading moves nothing but its own row.
// FAIL: an element appears, disappears or changes visibility; moves / resizes by more than 2px, sideways OR vertically;
//       or differs in a key computed style (display, position, overflow, font-size/weight, line-height, padding, gap,
//       flex-direction, grid track count, text-align, background painted or not). Colours are NOT compared: the theme
//       changes them on purpose. --loose <regex> is the explicit opt-out: a change on an element whose ancestor chain
//       matches is downgraded to WARN (size, position and style alike). Judge every WARN in the screenshots.
//       --loose-prop <regex> <property> is the narrow one (ruling P4-28): only a change of that ONE computed style on an
//       element whose chain matches is a WARN; any other style change, move or resize of it still FAILs.
//       The page heading, the diagnostic fold and the shell are not compared. The heading's own height change is taken out of
//       the y of #main's later children (reported as one WARN per screen), so a theme heading 2px shorter moves nothing.
// Baseline comparisons elsewhere (checkers failing at the start of plan 4) must key failures on the normalised failure
// MESSAGE (strip file:line, paths, timings), never file:line: later tasks edit those checkers and shift their lines.
// Scrollbars are hidden in every capture (ruling P4-10): the themed thin scrollbar is 10px narrower than the default
// one and would shift every full-width box. The page-scope root is keyed `body` without its classes (the `themed`
// class would otherwise rename every key).
import { chromium } from 'playwright';
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { STATES, serve, openState, settle } from './lib/page-states.mjs';

const [mode, page, a, b, ...rest] = process.argv.slice(2);
if (!STATES[page] || !['capture', 'diff'].includes(mode)) {
  console.error(`usage: capture <page> <dir> | diff <page> <before> <after> [--loose <regex>]...  (pages: ${Object.keys(STATES).join(', ')})`);
  process.exit(2);
}
const IGNORE = '.page-heading, details.core, .backdrop, .app-header, .input-footer, .skip-link, .update-center, script, style';
const VIEWPORTS = [{ width: 1280, height: 800 }, { width: 800, height: 560 }];
const NO_SCROLLBAR = '*{scrollbar-width:none !important} *::-webkit-scrollbar{display:none !important}';

async function capture(dir) {
  mkdirSync(dir, { recursive: true });
  const srv = await serve();
  const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
  const out = {};
  try {
    for (const state of STATES[page]) for (const viewport of VIEWPORTS) {
      const { page: p, errors, close } = await openState(browser, srv.base, state, { viewport });
      await p.addStyleTag({ content: NO_SCROLLBAR });
      await p.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
      await settle(p);
      const key = `${state.name}@${viewport.width}`;
      out[key] = await p.evaluate(({ scope, ignore }) => {
        const top = scope === 'page' ? document.body : document.getElementById('main');
        const rows = {};
        const name = (n) => `${n.tagName.toLowerCase()}${n.id ? `#${n.id}` : ''}${n.classList.length ? `.${[...n.classList].join('.')}` : ''}`;
        const walk = (node, keyPath, chain) => {
          [...node.children].forEach((child, i) => {
            if (child.matches(ignore)) return;
            const k = `${keyPath}/${child.tagName.toLowerCase()}[${i}]`;
            const c = `${chain} > ${name(child)}`;
            const r = child.getBoundingClientRect(), pr = node.getBoundingClientRect(), cs = getComputedStyle(child);
            const shown = cs.display !== 'none' && cs.visibility !== 'hidden' && (r.width > 0 || r.height > 0);
            const px = (v) => v.replace(/-?\d+\.\d+/g, (m) => String(Math.round(Number(m) * 10) / 10)); // sub-pixel noise: 40.00000762939453px
            const painted = cs.backgroundImage !== 'none' || !/^(transparent|rgba\(\s*\d+,\s*\d+,\s*\d+,\s*0\s*\))$/.test(cs.backgroundColor);
            const flex = /flex/.test(cs.display) ? cs.flexDirection : '';
            const grid = /grid/.test(cs.display) && cs.gridTemplateColumns !== 'none' ? `${cs.gridTemplateColumns.trim().split(/\s+/).length} cols` : '';
            const style = { display: cs.display, position: cs.position, 'overflow-x': cs.overflowX, 'overflow-y': cs.overflowY,
              'font-size': cs.fontSize, 'font-weight': cs.fontWeight, 'line-height': cs.lineHeight,
              'padding-top': cs.paddingTop, 'padding-right': cs.paddingRight, 'padding-bottom': cs.paddingBottom, 'padding-left': cs.paddingLeft,
              gap: cs.gap, 'flex-direction': flex, 'grid-columns': grid, 'text-align': cs.textAlign, painted: painted ? 'painted' : 'not painted' };
            for (const n of Object.keys(style)) style[n] = px(String(style[n]));
            rows[k] = { chain: c, shown, s: style, dx: Math.round(r.left - pr.left), dy: Math.round(r.top - pr.top), w: Math.round(r.width), h: Math.round(r.height) };
            if (shown) walk(child, k, c);
          });
        };
        const rootName = scope === 'page' ? 'body' : name(top);
        walk(top, rootName, rootName);
        return rows;
      }, { scope: state.scope, ignore: IGNORE });
      if (errors.length) out[key].__errors = errors;
      // The page heading is the theme's and is not compared, but its height moves every later child of #main. Its outer
      // height (unrounded) lets the diff take that move out of those children's y (Task 10).
      out[key].__heading = await p.evaluate(() => { const h = document.querySelector('#main > .page-heading'); if (!h) return null;
        const cs = getComputedStyle(h); return h.getBoundingClientRect().height + parseFloat(cs.marginTop) + parseFloat(cs.marginBottom); });
      const file = key.replace('@', '-');
      await p.screenshot({ path: path.join(dir, `${file}.png`) });
      await p.evaluate((scope) => { const m = scope === 'page' ? document.scrollingElement : document.getElementById('main'); if (m) m.scrollTop = m.scrollHeight; }, state.scope);
      await p.waitForTimeout(150);
      await p.screenshot({ path: path.join(dir, `${file}-end.png`) });
      await close();
    }
  } finally { await browser.close(); await srv.close(); }
  writeFileSync(path.join(dir, `${page}.json`), JSON.stringify(out, null, 1));
  console.log(`compare-page-layout: captured ${Object.keys(out).length} screen(s) of ${page} in ${dir}`);
}

function diff(beforeDir, afterDir, loose, looseProps = []) {
  const A = JSON.parse(readFileSync(path.join(beforeDir, `${page}.json`), 'utf8'));
  const B = JSON.parse(readFileSync(path.join(afterDir, `${page}.json`), 'utf8'));
  const fails = [], warns = [];
  for (const screen of Object.keys(A)) {
    const x0 = A[screen], y0 = B[screen];
    if (!y0) { fails.push(`${screen}: not captured after`); continue; }
    if (y0.__errors) fails.push(`${screen}: page errors after: ${y0.__errors.join(' | ')}`);
    // A direct child of #main below the (ignored) page heading moves with the heading's height; that move is the theme's.
    const hd = typeof x0.__heading === 'number' && typeof y0.__heading === 'number' ? y0.__heading - x0.__heading : 0;
    const headingMove = (k, x) => (hd && /^main#main\/[^/]+$/.test(k) && x.dy > 0 ? hd : 0);
    if (Math.abs(hd) > 0.01) warns.push(`${screen} .page-heading (theme's, not compared): outer height ${x0.__heading.toFixed(2)} → ${y0.__heading.toFixed(2)}; #main's children below it are compared without that move`);
    for (const k of Object.keys(x0)) {
      if (k.startsWith('__')) continue;
      const x = x0[k], y = y0[k];
      if (!y) { fails.push(`${screen} ${x.chain}: gone after`); continue; }
      if (x.shown !== y.shown) { fails.push(`${screen} ${x.chain}: ${x.shown ? 'hidden' : 'shown'} after`); continue; }
      if (!x.shown) continue;
      const side = Math.abs(x.dx - y.dx) > 2 || Math.abs(x.w - y.w) > 2;
      const vert = Math.abs(x.dy - (y.dy - headingMove(k, x))) > 2 || Math.abs(x.h - y.h) > 2;
      const line = `${screen} ${x.chain}: x ${x.dx}→${y.dx} w ${x.w}→${y.w} y ${x.dy}→${y.dy} h ${x.h}→${y.h}`;
      const isLoose = loose.some((re) => re.test(x.chain));
      if (side || vert) (isLoose ? warns : fails).push(line);
      if (x.s && y.s) {
        const changed = Object.keys(x.s).filter((n) => x.s[n] !== y.s[n]);
        const propLoose = (n) => looseProps.some(([re, prop]) => prop === n && re.test(x.chain));
        const say = (ns) => ns.map((n) => `${n} ${x.s[n]} → ${y.s[n]}`).join('; ');
        const strict = isLoose ? [] : changed.filter((n) => !propLoose(n)), soft = changed.filter((n) => !strict.includes(n));
        if (strict.length) fails.push(`${screen} ${x.chain}: STYLE ${say(strict)}`);
        if (soft.length) warns.push(`${screen} ${x.chain}: STYLE ${say(soft)}`);
      }
    }
    for (const k of Object.keys(y0)) if (!k.startsWith('__') && !x0[k]) fails.push(`${screen} ${y0[k].chain}: new after`);
  }
  for (const w of warns) console.log(`WARN  ${w}`);
  for (const f of fails) console.log(`FAIL  ${f}`);
  console.log(`compare-page-layout ${page}: ${fails.length ? 'FAIL' : 'PASS'} (${fails.length} fail, ${warns.length} warn)`);
  process.exit(fails.length ? 1 : 0);
}

if (mode === 'capture') await capture(path.resolve(a));
else {
  const loose = [], looseProps = [];
  for (let i = 0; i < rest.length; i++) {
    if (rest[i] === '--loose') loose.push(new RegExp(rest[++i]));
    else if (rest[i] === '--loose-prop') { looseProps.push([new RegExp(rest[i + 1]), rest[i + 2]]); i += 2; }
  }
  diff(path.resolve(a), path.resolve(b), loose, looseProps);
}
