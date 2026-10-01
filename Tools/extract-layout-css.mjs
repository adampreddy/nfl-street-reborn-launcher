// The layout a page gets from the retired stylesheets, written as one page stylesheet that loads after theme.css
// (spec §9, §11): layout properties kept exactly, every colour turned into a theme token (Tools/lib/colour-tokens.mjs),
// shadows, glows, filters, transitions and font families dropped. Run it while the page STILL loads the old sheets.
// Plan 4 Task 12 deleted those sheets: to regenerate a page file, run this on a checkout of 8fa5111 or earlier.
//     node Tools/extract-layout-css.mjs <page>[,<page>...] <out.css>
// Which rules: every rule of an old sheet that matches an element in one of the pages' states (Tools/lib/page-states.mjs),
// plus every rule whose class and id names all appear in the pages' own HTML and scripts, so a state the fixtures never
// reach keeps its layout too. Rules for what theme.css owns (the shell, what is inside the page heading, the hint bar,
// update cards, menus, the save bar) are left out; the heading's own box (its margins) is the page's and is kept.
// Declarations are read from the file text (the CSSOM empties var() shorthands that a later longhand partly overrides).
// A #main-scoped page also gets the old body text it inherited, and theme.css base layout the old sheets never set is
// reverted (BASE_UNDO). Every kept selector is prefixed `html:not(#_)`, so the old layout outranks
// theme.css's base rules (buttons, fields, headings) the way it did before. That prefix matches every page: the file
// must only ever be loaded by the page(s) it was made for.
// Plan-4 rulings applied here:
//   P4-11  only the RETIRED sheets are read (by file name). Never art/controllers/*/preview.css, which controller-preview.js
//          injects: its url()s are relative to its own folder and onboarding keeps loading it as it is.
//   P4-12  first run (body.page-setup) keeps the rules that hide the shell header, backdrop and page heading.
//   P4-13  an inset one-edge box-shadow with no blur (the selected-tab underline) is kept, its colour a token. Every
//          other shadow is dropped (no glow).
//   P4-14  Recovery keeps its diagnostic fold: the fold's rules are not "owned" there, and a hand-kept rule outranks
//          theme.css's `body.themed details.core{display:none}`.
//   scrollbar-gutter is layout and is kept; scrollbar colour and width are the theme's. After writing, the file is
//   proved on the first state rendered themed (theme.css + this file, body.themed): with the layout diff's
//   scrollbar neutralisation (Tools/compare-page-layout.mjs, P4-10) no box keeps a gutter, and the hand-kept rules hold.
import { chromium } from 'playwright';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { STATES, serve, openState, settle, ui } from './lib/page-states.mjs';
import { textToken, fillToken, lineToken } from './lib/colour-tokens.mjs';

const [pagesArg, outArg] = process.argv.slice(2);
const pages = String(pagesArg || '').split(',').filter(Boolean);
if (!pages.length || !outArg || pages.some((p) => !STATES[p])) {
  console.error(`usage: node Tools/extract-layout-css.mjs <page>[,<page>...] <out.css>   (pages: ${Object.keys(STATES).join(', ')})`);
  process.exit(2);
}
const out = path.resolve(outArg);
const scope = STATES[pages[0]][0].scope;

// P4-11: the stylesheets plan 4 retires (spec §11). Anything else a page loads (theme.css, pages/*.css, the controller
// art's preview.css) is never read.
const RETIRED = new Set(['styles.css', 'onboarding.css', 'launcher.css', 'product-ui.css', 'finish.css', 'onboarding-polish.css', 'live-view.css',
  'players.css', 'shell.css', 'play.css', 'stats.css', 'music.css', 'profile.css']);

// What theme.css owns. Onboarding (scope 'page') keeps its own sidebar: theme.css's shell skips body.page-setup.
// Recovery shows the diagnostic fold (legacy product-ui.css:76 exempted it, P4-14), so the fold is not owned there.
const FOLD = [/details\.core\b/, /\.out-head\b/, /#out\b/, /#clear\b/];
const OWNED = [/\.app-header\b/, /\.input-footer\b/, /#input-hints\b/, /\.input-hint\b/, /(^|[\s>+~(,])kbd\b/, /\.skip-link\b/, /\.backdrop\b/,
  /\.page-heading\b/, /\.eyebrow\b/, /\.update-/, /\.menu-pad-setup/, /\.menu(?![\w-])/,
  /\.save-lane\b/, /\.save-bar\b/, /\.nav-marker\b/, /\.pill\b/, /#bridge\b/, /\.look-/,
  ...(pages.includes('recovery') ? [] : FOLD),
  ...(scope === 'page' ? [] : [/\.shell-nav\b/, /\.brand\b/])];
// P4-12: first run hides the shell header, backdrop and page heading (launcher.css:62, 98, 128). Without these the
// themed setup page shows a second heading and main#main shifts left.
const NOT_OWNED = scope === 'page' ? [/^\.page-setup \.(page-heading|app-header|backdrop)$/] : [];
// A negation names what the rule does NOT touch: `button:not(.look-switch)` is a button rule, not a look rule.
const NOT = /:not\((?:[^()]|\([^()]*\))*\)/g;
// The page heading's own box (its margin and width, which place #screen below it) is the page's layout, as on the
// redesigned pages (pages/stats.css `.page-stats .page-heading{margin-bottom:24px}`); what is inside it (title, purpose
// line, eyebrow) is theme.css's.
const subject = (sel) => sel.replace(NOT, '').trim().split(/\s*[>+~]\s*|\s+/).pop();
const owned = (sel) => !NOT_OWNED.some((re) => re.test(sel.trim())) && !/\.page-heading\b/.test(subject(sel))
  && OWNED.some((re) => re.test(sel.replace(NOT, '')));
// The root, or a bare universal reset (`*`, `*::before`). `td.lv-who > *` is a rule for the cell's children, not a reset.
const subjectIsRoot = (sel) => {
  const parts = sel.trim().split(/\s*[>+~]\s*|\s+/);
  return /^(html|body|:root)([.#:[]|$)/.test(parts[parts.length - 1]) || (parts.length === 1 && /^\*([:[]|$)/.test(parts[0]));
};
const DYN = /::?(hover|focus-visible|focus-within|focus|active|visited|link|checked|disabled|enabled|placeholder-shown|placeholder|before|after|marker|selection|backdrop|first-line|first-letter|open|target|invalid|valid|indeterminate|-webkit-[\w-]+|-moz-[\w-]+)(\([^)]*\))?/g;
const COLOR = /^(color|background-color|border-(top|right|bottom|left)-color|outline-color|column-rule-color|text-decoration-color|caret-color|accent-color|fill|stroke)$/;
const TEXTISH = /^(color|fill|stroke|caret-color|text-decoration-color)$/;
const KEYWORD = /^(transparent|currentcolor|inherit|initial|unset|none)$/i;
// box-shadow is not here: keepShadow() decides (P4-13). scrollbar-gutter is not here: it is layout.
const DROP = /^(text-shadow|outline(-[\w-]+)?|filter|backdrop-filter|-webkit-backdrop-filter|mix-blend-mode|transition(-[\w-]+)?|font-family|font-stretch|font-feature-settings|font-kerning|font-optical-sizing|font-palette|font-synthesis(-[\w-]+)?|font-language-override|font-size-adjust|font-variation-settings|-webkit-font-smoothing|-webkit-tap-highlight-color|-webkit-text-fill-color|-webkit-text-stroke(-[\w-]+)?|scrollbar-(color|width)|color-scheme|mask(-[\w-]+)?|-webkit-mask(-[\w-]+)?|border-image(-[\w-]+)?|forced-color-adjust|print-color-adjust|accent-color)$/;

// theme.css body.themed base layout on plain elements that the old sheets left to the browser (found with the layout
// diff, Tools/compare-page-layout.mjs). Written first, so every copied rule of equal or higher specificity overrides it.
const BASE_UNDO = [
  ['theme.css gives classless buttons and a.button a fixed 38px height; the old ones grew with their text (min-height)',
    'html:not(#_) body :where(button:not([class]),button.primary:not(.btn),a.button){height:revert}'],
  // Found on Online's matchmaking states (Task 10, P4-14): its plain h3, p, ul and ol kept the browser's 1em margins.
  ['theme.css zeroes the margins of h1-h3, p, figure, ul and ol; the old sheets left the browser\'s where they set none (the page heading stays the theme\'s)',
    'html:not(#_) body :where(#main) :where(h1,h2,h3,p,figure,ul,ol):where(:not(.page-heading *)){margin:revert}'],
];

// Restores theme.css's body.themed base takes away and the old sheets never declared (plan Task 9 Step 3, "hand-kept").
// Written at the end of the file, below a `/* hand-kept: <why> */` comment each. Layout, plus the few colour states
// (selected, disputed, painted) the token mapping would otherwise lose (P4-27); colours are tokens only.
const HAND_KEPT = {
  recovery: [['the diagnostic fold stays visible on Recovery (P4-14; legacy product-ui.css:76 exempted page-recovery)',
    'html:not(#_) body.page-recovery details.core{display:block}']],
  // theme.css (plan 2) gives these first-run insets a fill because the old `background:var(--bg)` resolves to transparent
  // under the theme's alias. Before, theme.css won by source order; the html:not(#_) prefix now outranks it.
  onboarding: [['the inset fills theme.css gives first run (theme.css "inset fills that the --bg:transparent alias would otherwise erase")',
    'html:not(#_) body.shell.page-setup :is(.mapping-tabs,.graphics-tabs,.import-override-fields,.advanced-row select,.import-game-actions select){background-color:var(--glass-1)}\n'
    + 'html:not(#_) body.shell.page-setup .binding-editor{background-color:var(--surface-solid)}'],
  // Colour states the token mapping flattened (P4-27, Task 11), found by comparing every element's colours legacy vs themed.
  ['the step dots keep done (grey) apart from now (white): legacy onboarding-polish.css:8 done was var(--muted), which maps to --fg like now',
    'html:not(#_) .page-setup #steps span.done{background-color:var(--fg-2)}'],
  ['success and live-status text stays green (legacy onboarding-polish.css sage #c1d2ad, mapped to grey; as Online\'s .netplay-safe, P4-29 ruling)',
    'html:not(#_) :is(.setup-success,.controller-live,.detected-icon,.detected-check,.page-setup .recommendation-badge,.patch-complete .import-status){color:var(--ok)}'],
  ['a selected choice keeps its white outline and no red edge: legacy onboarding-polish.css:15 border-color:var(--focus) (#fff, mapped to --stroke-hi) and launcher.css:133 box-shadow:none (dropped as a shadow, so launcher.css:81\'s red inset showed)',
    'html:not(#_) .page-setup .choice:has(input:checked){border-top-color:var(--fg)!important;border-right-color:var(--fg)!important;border-bottom-color:var(--fg)!important;border-left-color:var(--fg)!important;box-shadow:none}'],
  ['a disabled Continue keeps 3:1 text and still reads as disabled: the old 0.4 dimming left 2.3:1 (P4-27)',
    'html:not(#_) .page-setup #next:disabled{opacity:.6}']],
  // game + live share pages/live.css (run as `game,live`). Colour states the token mapping flattened (P4-27, Task 9):
  // theme.css's classless-button sheen drew a line above every unselected tab; the old control and selected fills
  // both map to --glass-3, so the picked game no longer stood out; the disputed play's --muted edge became a faint line.
  live: [['tabs are bare text with an underline when selected (P4-13): no theme button sheen on them',
    'html:not(#_) body.shell #screen .lv-tabs button{box-shadow:none}'],
  ['the picked game stays distinct from the others (legacy live-view.css:104 selected fill + accent border)',
    'html:not(#_) body.shell #screen .lv-picker button:not([aria-pressed="true"]){background-color:var(--glass-1)}\n'
    + 'html:not(#_) body.shell #screen .lv-picker button[aria-pressed="true"]{border-color:var(--fg-3)}'],
  ['a disputed play keeps its light left edge (legacy live-view.css:110 border-left-color:var(--muted))',
    'html:not(#_) .lv-play.disputed{border-left-color:var(--fg-2)}']],
  // Online (Task 10, P4-27): the private room's "Personal saves protected" was a sage green (#b2c4a0, finish.css:98)
  // that maps to the grey --fg-2; it says something is safe, so it keeps the theme's one green.
  online: [['the room\'s "Personal saves protected" stays green (legacy finish.css .netplay-safe #b2c4a0)',
    'html:not(#_) .netplay-safe{color:var(--ok)}'],
  ['a disabled light primary ("Game in progress") keeps 3:1 text: the old 0.45 dimming left 2.6:1 (P4-27)',
    'html:not(#_) body.shell:not(.page-setup) button.primary:not(.btn):not(.look-switch):disabled{opacity:.6}']],
};

// ---- the pages' own sources: their HTML and every script they load, followed through imports ----
function sources() {
  const seen = new Set(), texts = [];
  const add = (file) => {
    if (seen.has(file) || !existsSync(file)) return;
    seen.add(file);
    const t = readFileSync(file, 'utf8');
    texts.push(t);
    for (const m of t.matchAll(/(?:\bfrom\s*|\bimport\s*\(\s*|\bimport\s+|<script\b[^>]*\bsrc=)["']([\w./-]+\.js)["']/g)) add(path.resolve(path.dirname(file), m[1]));
  };
  for (const p of pages) add(path.join(ui, `${p}.html`));
  return texts.join('\n');
}
const SRC = sources();
const named = (name) => new RegExp(`(?<![\\w-])${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![\\w-])`).test(SRC);
const tokensKnown = (sel) => {
  const names = [...sel.replace(NOT, '').replace(DYN, '').replace(/\[[^\]]*\]/g, '').matchAll(/[.#](-?[_a-zA-Z][\w-]*)/g)].map((m) => m[1]);
  return names.length > 0 && names.every(named);
};
// `primary` is named by app-shell.js (`.matches('.primary')`) on every page, so the old primary-button rules passed the
// named-in-source test on pages that never make one (Recovery, game, live). A selector part that needs a non-.btn
// `.primary` is kept only when it matched in a state or the page's sources create such a class (Task 10 carry).
const createsPrimary = /\bclass(?:Name)?\s*[:=]\s*(["'`])(?![^"'`]*\bbtn\b)[^"'`]*\bprimary\b[^"'`]*\1|\bel\(\s*["']\w+["']\s*,\s*["'][^"']*\bprimary\b/.test(SRC);
const needsPrimary = (sel) => /\.primary\b/.test(sel.replace(NOT, ''));
const prefix = (sel) => (/^html\b/.test(sel) ? sel.replace(/^html/, 'html:not(#_)') : /^:root\b/.test(sel) ? sel.replace(/^:root/, ':root:not(#_)') : `html:not(#_) ${sel}`);

// ---- each retired sheet's style rules as WRITTEN, in the order the CSSOM walk below visits them ----
// The CSSOM cannot be trusted for declarations: a shorthand holding var() (`border:1px solid var(--line)`) that a
// later longhand partly overrides (`border-left:3px solid …`) serialises its remaining longhands EMPTY, and the
// top, right and bottom borders vanish. So declarations come from the file text; the CSSOM supplies the order.
// Style rules only (at any @media/@supports depth); @keyframes, @font-face and other at-rules are skipped, as the walk does.
function rawRules(file) {
  const t = readFileSync(path.join(ui, file), 'utf8').replace(/\/\*[\s\S]*?\*\//g, ' ');
  const rules = [];
  let i = 0;
  const skipString = (q) => { i++; while (i < t.length && t[i] !== q) i += t[i] === '\\' ? 2 : 1; i++; };
  const readUntil = (stops) => {
    const start = i; let depth = 0;
    while (i < t.length) {
      const c = t[i];
      if (c === '"' || c === "'") { skipString(c); continue; }
      if (c === '(' || c === '[') depth++;
      else if (c === ')' || c === ']') depth--;
      else if (!depth && stops.includes(c)) break;
      i++;
    }
    return t.slice(start, i);
  };
  const skipBlock = () => { let depth = 0; do { const c = t[i]; if (c === '"' || c === "'") { skipString(c); continue; } if (c === '{') depth++; if (c === '}') depth--; i++; } while (depth && i < t.length); };
  const block = () => {
    while (i < t.length) {
      while (/\s/.test(t[i] || '')) i++;
      if (i >= t.length) return;
      if (t[i] === '}') { i++; return; }
      const prelude = readUntil('{;}').trim();
      if (t[i] === ';') { i++; continue; }
      if (t[i] === '}') continue;
      if (/^@(media|supports)\b/i.test(prelude)) { i++; block(); continue; }
      if (prelude.startsWith('@')) { skipBlock(); continue; }
      i++;
      const body = readUntil('}');
      i++;
      rules.push({ sel: prelude, body });
    }
  };
  block();
  return rules;
}
const RAW = Object.fromEntries([...RETIRED].filter((f) => existsSync(path.join(ui, f))).map((f) => [f, rawRules(f)]));

// ---- in the page: every old rule, whether each selector matches something on screen, its declarations as longhands ----
async function collect(page) {
  return page.evaluate(({ scopeMain, dynSource, colorSource, retired, raw }) => {
    const DYNRE = new RegExp(dynSource, 'g');
    const COLORRE = new RegExp(colorSource);
    const main = document.getElementById('main');
    const inScope = (n) => !scopeMain || n === main || Boolean(main && main.contains(n)) || Boolean(n.closest('dialog:not(.menu):not(.update-dialog):not(.menu-pad-setup-dialog)'));
    const hit = (sel) => {
      const s = sel.replace(DYNRE, '').replace(/:not\(\s*\)/g, '').trim() || '*';
      try { return [...document.querySelectorAll(s)].some(inScope); } catch { return false; }
    };
    const split = (text, sep) => {
      const parts = []; let depth = 0, cur = '';
      for (const ch of text) {
        if (ch === '(' || ch === '[') depth++;
        if (ch === ')' || ch === ']') depth--;
        if (ch === sep && !depth) { parts.push(cur); cur = ''; } else cur += ch;
      }
      parts.push(cur);
      return parts.map((s) => s.trim()).filter(Boolean);
    };
    const probe = document.createElement('div');
    document.body.append(probe);
    const rgba = (v) => {
      if (/^(currentcolor|inherit|initial|unset|revert)$/i.test(String(v).trim())) return null;
      probe.style.color = '';
      probe.style.color = v;
      if (!probe.style.color) return null;
      const m = getComputedStyle(probe).color.match(/[\d.]+/g);
      if (!m) return null;
      const n = m.map(Number);
      return [n[0], n[1], n[2], n.length > 3 ? n[3] : 1];
    };
    // A box-shadow as the browser computes it: "rgb(224, 39, 46) 0px -2px 0px 0px inset" (P4-13 reads it in Node).
    const shadow = (v) => {
      probe.style.boxShadow = '';
      probe.style.boxShadow = v;
      return probe.style.boxShadow ? getComputedStyle(probe).boxShadow : null;
    };
    const vars = getComputedStyle(document.body);
    const resolve = (v) => {
      let s = v;
      for (let i = 0; i < 10 && /var\(/.test(s); i++) {
        s = s.replace(/var\(\s*(--[\w-]+)\s*(?:,\s*([^()]*(?:\([^()]*\)[^()]*)*))?\)/g, (m, name, fb) => vars.getPropertyValue(name).trim() || (fb || '').trim());
      }
      return s;
    };
    const expand = (prop, value) => {
      const st = document.createElement('div').style;
      st.setProperty(prop, value);
      const res = [];
      for (let i = 0; i < st.length; i++) res.push([st[i], st.getPropertyValue(st[i])]);
      return res;
    };
    // P4-11: by file name, and never anything under art/ or pages/.
    const sheets = [...document.styleSheets].filter((s) => {
      if (!s.href) return false;
      const p = new URL(s.href).pathname;
      return !/\/(art|pages)\//.test(p) && retired.includes(p.split('/').pop());
    });
    const rules = [], keyframes = {};
    const unmatched = [];
    const norm = (sel) => sel.toLowerCase().replace(/::?(before|after|first-line|first-letter|marker|placeholder|selection|backdrop)\b/g, '::$1').replace(/["'\s]/g, '').replace(/\*(?=[:.#[])/g, '');
    for (const sheet of sheets) {
      const file = sheet.href.split('/').pop().split('?')[0];
      let ord = 0, rp = 0;
      const src = raw[file] || [];
      // The written declarations of this CSSOM rule: the next raw rule with the same selector (a rule the browser
      // dropped as invalid is skipped). None found: the CSSOM's own text, and the miss is counted.
      const written = (r) => {
        const want = norm(r.selectorText);
        for (let k = rp; k < Math.min(src.length, rp + 8); k++) if (norm(src[k].sel) === want) { rp = k + 1; return src[k].body; }
        unmatched.push(`${file}: ${r.selectorText}`);
        return r.style.cssText;
      };
      const walk = (list, media) => {
        for (const r of list) {
          if (r instanceof CSSKeyframesRule) {
            keyframes[r.name] = { css: r.cssText, safe: [...r.cssRules].every((k) => [...k.style].every((p) => /^(opacity|transform|translate|scale|rotate|animation-timing-function)$/.test(p))) };
            continue;
          }
          if (r instanceof CSSMediaRule || r instanceof CSSSupportsRule) {
            walk(r.cssRules, [...media, `@${r instanceof CSSMediaRule ? 'media' : 'supports'} ${r.conditionText}`]);
            continue;
          }
          if (!(r instanceof CSSStyleRule)) continue;
          const key = `${file}:${ord++}`;
          const parts = split(r.selectorText, ',').map((sel) => ({ sel, hit: hit(sel) }));
          const decls = [];
          for (const d of split(written(r), ';')) {
            const i = d.indexOf(':');
            if (i < 0) continue;
            const prop = d.slice(0, i).trim().toLowerCase();
            let value = d.slice(i + 1).trim();
            const important = /!\s*important$/i.test(value);
            value = value.replace(/!\s*important$/i, '').trim();
            if (prop.startsWith('--')) continue;
            for (const [p, v] of expand(prop, resolve(value))) {
              decls.push({ group: prop, prop: p, value: v, important, rgba: COLORRE.test(p) ? rgba(v) : null, shadow: p === 'box-shadow' ? shadow(v) : null });
            }
          }
          rules.push({ key, media, parts, decls });
        }
      };
      walk([...sheet.cssRules], []);
    }
    probe.remove();
    // The text the old body handed down to #main. theme.css's body.themed is `500 14px/1.45`, and body rules are not
    // copied for a #main-scoped page, so without this every inherited weight and line height changes.
    const b = getComputedStyle(document.body);
    const lh = b.lineHeight === 'normal' ? 'normal' : String(Math.round((parseFloat(b.lineHeight) / parseFloat(b.fontSize)) * 1000) / 1000);
    const rootText = { 'font-size': b.fontSize, 'font-weight': b.fontWeight, 'line-height': lh, 'letter-spacing': b.letterSpacing, 'font-style': b.fontStyle, 'text-transform': b.textTransform };
    return { unmatched, rootText, rules, keyframes, sheets:sheets.map((s) => new URL(s.href).pathname.split('/').pop()) };
  }, { scopeMain: scope === 'main', dynSource: DYN.source, colorSource: COLOR.source, retired: [...RETIRED], raw: RAW });
}

// ---- in Node: which declarations survive, as what ----
const rewriteUrls = (v) => v.replace(/url\(\s*(["']?)([^"')]+)\1\s*\)/g, (m, q, u) => {
  const local = u.replace(/^https?:\/\/127\.0\.0\.1:\d+\//, '');
  if (/^(data:|https?:|\/|#)/.test(local)) return m;
  return `url("${path.relative(path.dirname(out), path.join(ui, local)).split(path.sep).join('/')}")`;
});
// Dark text only ever sat on a light fill (the old white primary button: #252722 on #eceee4). The nearest text token is
// --fg-3, about 3.6:1 on the --fg fill it maps to; the theme's dark surface keeps it dark (about 15:1). Task 10 carry.
const INK_DARK = ([r, g, b, a]) => a >= 0.6 && (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255 < 0.25;
function colourValue(d) {
  if (!d.rgba) return KEYWORD.test(d.value.trim()) ? d.value.trim().toLowerCase() : null;
  if (TEXTISH.test(d.prop) && INK_DARK(d.rgba)) return 'var(--surface-solid)';
  if (TEXTISH.test(d.prop)) return textToken(d.rgba);
  if (d.prop === 'background-color') return fillToken(d.rgba);
  return lineToken(d.rgba);
}
// P4-13: one inset shadow along one edge with no blur and no spread is an underline (or side rule), not a glow. It
// stays, its colour a token. Anything else (rings, blurs, several shadows) is dropped.
function keepShadow(d) {
  const s = d.shadow;
  if (!s || s === 'none' || /,(?![^(]*\))/.test(s) || !/\binset\b/.test(s)) return null;
  const colour = s.match(/rgba?\([^)]*\)/);
  const nums = s.replace(/rgba?\([^)]*\)/, '').match(/-?[\d.]+px/g)?.map((n) => parseFloat(n)) || [];
  if (!colour || nums.length < 2) return null;
  const [x, y, blur = 0, spread = 0] = nums;
  if (blur !== 0 || spread !== 0 || (x !== 0) === (y !== 0)) return null;
  const c = colour[0].match(/[\d.]+/g).map(Number);
  return `inset ${x}px ${y}px ${fillToken([c[0], c[1], c[2], c.length > 3 ? c[3] : 1])}`;
}
function keep(d, imageFromShorthand) {
  // `outline:none` is not a look but its absence: the setup title is focused by script (tabindex=-1) and drew no ring.
  if (d.prop === 'outline-style' && d.value === 'none') return 'none';
  if (DROP.test(d.prop)) return null;
  if (d.prop === 'box-shadow') return keepShadow(d);
  if (d.prop === 'background-image') return /url\(/.test(d.value) && !/gradient/.test(d.value) ? rewriteUrls(d.value) : null;
  if (d.prop.startsWith('background-') && d.prop !== 'background-color') return d.group === d.prop || imageFromShorthand ? d.value : null;
  if (COLOR.test(d.prop)) return colourValue(d);
  if (d.prop === 'font-weight') {
    const n = d.value === 'bold' ? 700 : d.value === 'normal' ? 400 : Number(d.value);
    return Number.isFinite(n) ? String(Math.min(700, n)) : d.value;
  }
  return /url\(/.test(d.value) ? rewriteUrls(d.value) : d.value;
}

const srv = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
const merged = new Map();
const read = new Set();
let keyframes = {}, rootText = null;
const unmatched = new Set();
try {
  for (const p of pages) for (const state of STATES[p]) {
    const { page, close } = await openState(browser, srv.base, state);
    const got = await collect(page);
    await close();
    rootText ||= got.rootText;
    got.sheets.forEach((s) => read.add(s));
    got.unmatched.forEach((u) => unmatched.add(u));
    keyframes = { ...keyframes, ...got.keyframes };
    for (const r of got.rules) {
      const was = merged.get(r.key);
      if (!was) merged.set(r.key, r);
      else r.parts.forEach((part, i) => { if (part.hit && was.parts[i]) was.parts[i].hit = true; });
    }
  }
} catch (e) { await browser.close(); await srv.close(); throw e; }

const lines = [], used = new Set(), oldFonts = new Set();
let kept = 0, left = 0;
// First, at the lowest specificity the prefix allows ((1,0,2): every copied rule beats it), the old body text for the
// scope: #main and the page's own dialogs inherit it as they did. Onboarding (scope 'page') copies its body rules instead.
if (scope === 'main' && rootText) {
  lines.push(`/* the old body text, inherited by #main and the page's dialogs (theme.css body.themed is 500 14px/1.45) */`,
    `html:not(#_) body :where(#main,dialog:not(.menu):not(.update-dialog):not(.menu-pad-setup-dialog)){${Object.entries(rootText).map(([k, v]) => `${k}:${v}`).join(';')}}`);
}
// Then, at the same lowest specificity, the layout theme.css's body.themed base sets on plain elements and the old
// sheets never set, put back to the browser's own (`revert`): a copied rule that sets the property still wins.
for (const [why, rule] of BASE_UNDO) lines.push(`/* ${why} */`, rule);
for (const r of merged.values()) {
  const sels = r.parts.filter((p) => !owned(p.sel) && (scope === 'page' || !subjectIsRoot(p.sel)) && (p.hit || (tokensKnown(p.sel) && (createsPrimary || !needsPrimary(p.sel))))).map((p) => prefix(p.sel));
  if (!sels.length) { left++; continue; }
  const imageFromShorthand = r.decls.some((d) => d.group === 'background' && d.prop === 'background-image' && /url\(/.test(d.value) && !/gradient/.test(d.value));
  const names = (r.decls.find((d) => d.prop === 'animation-name')?.value || 'none').split(',').map((s) => s.trim()).filter((n) => n && n !== 'none');
  const animOk = names.length > 0 && names.every((n) => keyframes[n]?.safe);
  if (r.decls.some((d) => d.prop === 'font-family' && /anton|saira/i.test(d.value))) sels.forEach((s) => oldFonts.add(s));
  const body = [];
  for (const d of r.decls) {
    const v = /^animation/.test(d.prop) ? (animOk ? d.value : null) : keep(d, imageFromShorthand);
    if (v === null || v === undefined || v === '') continue;
    body.push(`${d.prop}:${v}${d.important ? '!important' : ''}`);
  }
  if (!body.length) { left++; continue; }
  // A light (--fg) fill came from an old `background:` shorthand, which also cleared any image. theme.css paints its
  // primary button with a gradient image, so without this the old white primary showed red with dark ink (Task 10).
  if (body.some((x) => /^background-color:var\(--fg\)/.test(x)) && !body.some((x) => x.startsWith('background-image:'))) body.push('background-image:none');
  if (animOk) names.forEach((n) => used.add(n));
  kept++;
  let text = `${sels.join(',')}{${body.join(';')}}`;
  for (const m of [...r.media].reverse()) text = `${m}{${text}}`;
  lines.push(text);
}
for (const n of used) lines.push(keyframes[n].css);
const handKept = pages.flatMap((p) => HAND_KEPT[p] || []);
for (const [why, rule] of handKept) lines.push(`/* hand-kept: ${why} */`, rule);

const these = pages.length > 1 ? 'these pages' : 'this page';
const head = [
  `/* ${pages.map((p) => `${p}.html`).join(' + ')}: the layout the retired stylesheets gave ${these} (spec §9, §11),`,
  '   made by Tools/extract-layout-css.mjs and checked with Tools/compare-page-layout.mjs.',
  '   Layout only: colours are theme tokens; shadows, glows, filters, transitions and font families are gone.',
  `   Every selector starts with html:not(#_) so the old layout outranks theme.css's base rules: load this file on ${these} only.`,
  ...(scope === 'main' ? ['   P4-29: the old body.shell h1 rule (28px/600) also styles the page title; kept on purpose, these pages are not redesigned (spec §9).'] : []),
  ...(oldFonts.size ? [`   The old font of these was Anton or Saira (now Inter): ${[...oldFonts].join(', ')}`] : []),
  '*/',
].join('\n');
const css = `${head}\n${lines.join('\n')}\n`;
writeFileSync(out, css);
console.log(`extract-layout-css: ${kept} rule(s) kept, ${left} left out, ${used.size} keyframe(s) -> ${path.relative(process.cwd(), out)}`);
console.log(`extract-layout-css: read ${[...read].join(', ')}; ${unmatched.size} rule(s) not found in the file text (CSSOM text used)${[...unmatched].map((u) => `\n  ${u}`).join('')}`);

// ---- proof on a themed render: the first state with theme.css + this file and body.themed, nothing else ----
// The layout diff hides every scrollbar (P4-10); kept `scrollbar-gutter` must not survive that, or every full-width box
// would differ by the gutter. The hand-kept rules (and the P4-12 hides on first run) must hold.
const NO_SCROLLBAR = '*{scrollbar-width:none !important} *::-webkit-scrollbar{display:none !important}';
const proofCss = `/__extract-layout-proof.css`;
let failed = false;
try {
  const state = STATES[pages[0]][0];
  const { page, errors, close } = await openState(browser, srv.base, state);
  const file = state.url.split('?')[0];
  await page.route((u) => new URL(u).pathname === `/${file}`, async (route) => {
    const res = await route.fetch();
    let html = await res.text();
    html = html.replace(/[ \t]*(?:<link rel="stylesheet" href="[^"]+">)+[ \t]*\r?\n/g, '');
    html = html.replace('</head>', `<link rel="stylesheet" href="theme.css"><link rel="stylesheet" href="${proofCss.slice(1)}"></head>`);
    html = html.replace(/<body class="([^"]*)"/, (m, cls) => `<body class="${cls} themed"`);
    await route.fulfill({ response: res, body: html, headers: { ...res.headers(), 'content-type': 'text/html' } });
  });
  await page.route((u) => new URL(u).pathname === proofCss, (route) => route.fulfill({ status: 200, contentType: 'text/css', body: css }));
  await page.reload();
  await page.waitForFunction(() => { const s = document.getElementById('screen'); return s && !s.querySelector('.loading'); }, null, { timeout: 8000 }).catch(() => {});
  await settle(page);
  const gutters = () => page.evaluate((scopeMain) => {
    const top = scopeMain ? document.getElementById('main') : document.body;
    const boxes = [top, ...top.querySelectorAll('*')].filter((n) => /auto|scroll/.test(getComputedStyle(n).overflowY) || getComputedStyle(n).scrollbarGutter !== 'auto');
    return boxes.map((n) => { const cs = getComputedStyle(n); return { n: n.tagName.toLowerCase() + (n.id ? `#${n.id}` : ''), g: n.offsetWidth - n.clientWidth - parseFloat(cs.borderLeftWidth) - parseFloat(cs.borderRightWidth) }; });
  }, state.scope === 'main');
  const sheetsNow = await page.evaluate(() => [...document.styleSheets].map((s) => s.href && new URL(s.href).pathname.split('/').pop()).filter(Boolean));
  const before = await gutters();
  await page.addStyleTag({ content: NO_SCROLLBAR });
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  const after = await gutters();
  const left = after.filter((b) => b.g > 0);
  const legacyLeft = sheetsNow.filter((s) => RETIRED.has(s) && s !== path.basename(out));
  console.log(`extract-layout-css: proof ${state.name} themed (${sheetsNow.join(' + ')}): ${before.filter((b) => b.g > 0).length} box(es) with a gutter, ${left.length} once scrollbars are neutralised`);
  if (left.length) { failed = true; console.error(`  FAIL gutter survives neutralisation: ${left.map((b) => `${b.n} ${b.g}px`).join(', ')}`); }
  if (legacyLeft.length) { failed = true; console.error(`  FAIL the themed proof still loads ${legacyLeft.join(', ')}`); }
  const holds = await page.evaluate(() => {
    const shown = (sel) => [...document.querySelectorAll(sel)].some((n) => getComputedStyle(n).display !== 'none');
    return { fold: document.querySelector('details.core') ? shown('details.core') : null,
      setupHides: document.body.classList.contains('page-setup') ? ['.page-heading', '.app-header', '.backdrop'].filter(shown) : null };
  });
  if (pages.includes('recovery')) {
    console.log(`extract-layout-css: proof recovery diagnostic fold ${holds.fold ? 'visible' : 'HIDDEN'} (P4-14)`);
    if (!holds.fold) failed = true;
  }
  if (holds.setupHides) {
    console.log(`extract-layout-css: proof first run hides header/backdrop/heading: ${holds.setupHides.length ? `FAIL shown ${holds.setupHides.join(', ')}` : 'yes'} (P4-12)`);
    if (holds.setupHides.length) failed = true;
  }
  if (errors.length) console.log(`  (page errors in the themed proof: ${errors.join('; ')})`);
  await close();
} finally { await browser.close(); await srv.close(); }
if (failed) process.exit(1);
