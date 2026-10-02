// The Online page redesign (docs/superpowers/specs/2026-10-02-online-page-redesign-design.md): every mockup state on
// the real page, from the fixed states in lib/page-states.mjs (frozen clock 2026-09-30 20:00 UTC, no network), plus the
// static rules the page's files must keep. Fails on a missing text or control, a broken layout rule, a sideways scroll
// or any page error. Screenshots land in target/online-page.
//     node Tools/check-online-page.mjs
import { chromium } from 'playwright';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { STATES, root, serve, openState } from './lib/page-states.mjs';

const ui = path.join(root, 'crates/reborn-ui/ui');
const out = path.join(root, 'target/online-page');
mkdirSync(out, { recursive: true });
let failures = 0;
const fail = (name, msg) => { failures++; console.log(`FAIL ${name}: ${msg}`); };
const ok = (name) => console.log(`ok   ${name}`);
const calls = (page, cmd) => page.evaluate((c) => window.__calls.filter((x) => x.cmd === c).map((x) => x.args), cmd);

// ---- static: tokens, the sweep, the art ----
{
  const theme = readFileSync(path.join(ui, 'theme.css'), 'utf8');
  const design = readFileSync(path.join(ui, 'DESIGN.md'), 'utf8');
  const problems = [];
  for (const t of ['--tile-scrim', '--vs-scrim', '--pill-bg', '--lock-bg', '--red-wash']) {
    if (!new RegExp(`${t}\\s*:`).test(theme)) problems.push(`theme.css does not define ${t}`);
    if (!design.includes(`\`${t}\``)) problems.push(`DESIGN.md does not name ${t}`);
  }
  if (!/\.sweep::after\{[^}]*animation:sweep /.test(theme)) problems.push('theme.css has no .sweep loop');
  if (!design.includes('`.sweep`')) problems.push('DESIGN.md does not name .sweep');
  // A JPEG's width from its first SOFn marker (no image library in Tools).
  const jpegWidth = (buf) => {
    let i = 2;
    while (i + 9 < buf.length && buf[i] === 0xff) {
      const m = buf[i + 1], len = buf.readUInt16BE(i + 2);
      if (m >= 0xc0 && m <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(m)) return buf.readUInt16BE(i + 7);
      i += 2 + len;
    }
    return null;
  };
  for (const f of ['ranked', 'casual', 'private', 'matchup']) {
    const p = path.join(ui, `art/online/${f}.jpg`);
    if (!existsSync(p)) { problems.push(`art/online/${f}.jpg is missing`); continue; }
    const w = jpegWidth(readFileSync(p));
    if (!w || w > 1100) problems.push(`art/online/${f}.jpg is ${w}px wide (a display copy is at most 1100)`);
  }
  if (problems.length) fail('static', problems.join('; ')); else ok('static');
}

// ---- every state ----
const EXPECT = {
  'online-layout': {
    text: ['Online', 'Ranked, casual, or a private game with a friend.'],
    check: async (page) => {
      const p = [];
      const lb = await page.locator('#leaderboard-slot').boundingBox();
      if (!lb || Math.round(lb.width) !== 316) p.push(`the leaderboard column is ${lb?.width}px wide, not 316`);
      const mode = await page.locator('#mode-slot').boundingBox(), live = await page.locator('#live-slot').boundingBox();
      if (!mode || !live || mode.y + mode.height > live.y) p.push('the mode area does not sit above Live now');
      if (await page.locator('#direct-dialog').evaluate((d) => d.open)) p.push('the direct connection dialog is open on load');
      // From Task 4 on, the links line shows under Private room only (ruling 6): pick it when the link is hidden.
      if (!(await page.locator('#direct-open').isVisible())) await page.locator('[data-key="mode-private"]').click();
      await page.locator('#direct-open').click();
      if (!(await page.locator('#direct-dialog').evaluate((d) => d.open))) p.push('Direct connection did not open its dialog');
      if (!(await page.getByRole('button', { name: 'Join game', exact: true }).isVisible())) p.push("the dialog does not hold online.js's form");
      await page.locator('#direct-close').click();
      if (await page.locator('#direct-dialog').evaluate((d) => d.open)) p.push('Close did not close the dialog');
      return p;
    },
  },
  'online-small': {
    check: async (page) => {
      const p = [];
      const mode = await page.locator('#mode-slot').boundingBox(), lb = await page.locator('#leaderboard-slot').boundingBox();
      if (!mode || mode.width < 300) p.push(`the mode area is ${mode?.width}px wide at 800x560`);
      if (!lb || !mode || lb.y < mode.y + mode.height) p.push('the leaderboard does not drop below the mode area at 800x560');
      return p;
    },
  },
  'online-netplay-invite': {
    text: ['Invite received: room K7QP2M9X.'],
    check: async (page) => {
      const p = [];
      if (!(await page.locator('#direct-dialog').evaluate((d) => d.open))) p.push('the invite did not open the Direct connection dialog');
      const code = await page.getByLabel('Host code', { exact: true }).inputValue().catch(() => null);
      if (code !== 'K7QP2M9X') p.push(`the dialog's host code is ${JSON.stringify(code)}, not the invite's K7QP2M9X`);
      if (!(await page.getByRole('button', { name: 'Join game', exact: true }).isVisible())) p.push('Join game is not on screen');
      return p;
    },
  },
  // (states are added above this line)
};

const byName = Object.fromEntries((STATES.online || []).map((s) => [s.name, s]));
if (Object.keys(EXPECT).length) {
  const server = await serve();
  const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
  try {
    for (const [name, exp] of Object.entries(EXPECT)) {
      const state = byName[name];
      if (!state) { fail(name, 'no such state in Tools/lib/page-states.mjs'); continue; }
      const { page, errors, close } = await openState(browser, server.base, state, { pad: !!state.pad, init: state.init ?? null, viewport: state.viewport });
      const problems = [];
      try {
        const text = (await page.locator('body').innerText()).toLowerCase();
        for (const t of exp.text ?? []) if (!text.includes(t.toLowerCase())) problems.push(`missing text "${t}"`);
        for (const t of exp.absent ?? []) if (text.includes(t.toLowerCase())) problems.push(`should not show "${t}"`);
        if (exp.check) problems.push(...(await exp.check(page)));
        if (await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)) problems.push('the page scrolls sideways');
        await page.screenshot({ path: path.join(out, `${name}.png`) });
      } catch (e) {
        problems.push(`threw: ${e.message.split('\n')[0]}`);
      } finally {
        await close();
      }
      if (errors.length) problems.push(`page errors: ${errors.join(' | ')}`);
      if (problems.length) fail(name, problems.join('; ')); else ok(name);
    }
  } finally {
    await browser.close();
    await server.close();
  }
}
if (failures) { console.error(`check-online-page: FAIL (${failures})`); process.exit(1); }
console.log(`check-online-page: PASS (${Object.keys(EXPECT).length} state(s))`);
