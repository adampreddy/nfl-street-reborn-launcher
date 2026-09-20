// Play never opens the game on a controller that cannot press Start without saying so.
//
// Reported 2026-09-19: "after i open the app i cant get pass the press start screen". The
// launcher has always known, before the game opened, which controls could not be pressed. It
// wrote that to a log and started the game anyway. The answer to Play can now say the
// controller needs setting up, and this is the page doing something with that answer:
// telling the player, in words that are not a failure, and giving them both ways forward.
//
// Ablation: HOME_JS=<home.js before this change> must FAIL.
import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import assert from 'node:assert/strict';

const root = path.resolve(import.meta.dirname, '..');
const ui = path.join(root, 'crates/reborn-ui/ui');
const homeOverride = process.env.HOME_JS || null;

// The same "everything is fine" answers the shell-pages checker uses, so the Play page comes
// up ready to play and the only thing under test is what it does with the launch answer.
const source = readFileSync(path.join(root, 'Tools/check-shell-pages.mjs'), 'utf8');
const fixture = vm.runInNewContext(source.slice(source.indexOf('const FULL ='), source.indexOf('// Every field null')) + '\nFULL');
Object.assign(fixture, {
  session_active: false,
  onboarding_progress: { complete: true, next: { kind: 'already_complete' }, summary: {} },
  update_check: { state: 'current', current: '3.0.22' }, update_activity: { downloading: false, ready: null }, update_policy: { automatic: false },
  content_status: { phase: 'up_to_date' }, content_check: { phase: 'up_to_date' },
});

const mime = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.svg': 'image/svg+xml', '.woff2': 'font/woff2' };
const server = createServer((req, res) => {
  try {
    const name = decodeURIComponent(new URL(req.url, 'http://test').pathname);
    const p = path.resolve(ui, '.' + name);
    if (!p.startsWith(ui + path.sep)) throw Error('outside');
    res.writeHead(200, { 'Content-Type': mime[path.extname(p)] || 'application/octet-stream' });
    res.end(readFileSync(name === '/home.js' && homeOverride ? homeOverride : p));
  } catch (_) { res.writeHead(404); res.end(); }
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}`;

const MESSAGE = 'Your controller needs a one-time setup before it can be used in the game: Start and A would do nothing yet. Open Controllers and choose Set up this controller. It takes about a minute, and you only do it once.';

const browser = await chromium.launch({ channel: 'msedge', headless: true });
const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
await context.addInitScript(({ data, message }) => {
  window.calls = [];
  window.__TAURI__ = { event: { listen: async () => () => {} }, core: { invoke: async (cmd, args = {}) => {
    window.calls.push({ cmd, args: JSON.parse(JSON.stringify(args)) });
    if (cmd === 'launch') {
      const blank = { user_dir_note: null, minutes: null, needs_recovery: false, pads: [], pads_authoritative: true, mods_note: null, session_id: null, stats_note: null, cards_note: null };
      // The real command: asked first, it refuses and says why; told to go on, it starts.
      return args.playAnyway
        ? { ...blank, started: true, message: 'Session ended normally.', minutes: 1, controller_setup: null }
        : { ...blank, started: false, message, controller_setup: { device: 'WGInput/0/Pro Controller', controls: ['Start', 'A'] } };
    }
    if (cmd === 'session_active') return false;
    if (cmd === 'ui_notices') return [];
    if (cmd in data) return data[cmd];
    return null;
  } } };
}, { data: fixture, message: MESSAGE });

const page = await context.newPage();
const errors = []; page.on('pageerror', e => errors.push(e.message));
const failures = [];
const step = async (name, body) => {
  try { await body(); console.log('ok    ' + name); }
  catch (e) { failures.push(name); console.log('FAIL  ' + name + '\n      ' + String(e.message || e).split('\n')[0]); }
};
const launches = () => page.evaluate(() => window.calls.filter(c => c.cmd === 'launch').map(c => c.args));

await page.goto(base + '/index.html');
await page.locator('#play-now').waitFor({ timeout: 15000 });

await step('Play asks first: the launch is sent with playAnyway false', async () => {
  await page.locator('#play-now').click();
  await page.waitForFunction(() => window.calls.some(c => c.cmd === 'launch'), null, { timeout: 5000 });
  const sent = await launches();
  assert.equal(sent.length, 1);
  assert.equal(sent[0].playAnyway, false, 'the key must be sent, and false: ' + JSON.stringify(sent[0]));
});

await step('the player is told the controller needs setting up, and it is not called a failure', async () => {
  const notice = page.locator('.play-controller-setup');
  await notice.waitFor({ timeout: 5000 });
  const text = await notice.innerText();
  assert.match(text, /needs a one-time setup/i);
  assert.match(text, /Start and A would do nothing yet/);
  assert.doesNotMatch(await page.locator('.play-state-title').innerText(), /could not start/i, 'a setup to offer is not a game that could not start');
});

await step('both ways forward are offered: set it up, or play anyway', async () => {
  const setup = page.locator('.play-controller-setup a', { hasText: 'Set up controller' });
  assert.equal(await setup.getAttribute('href'), 'controls.html');
  assert.equal(await page.locator('#play-anyway').isVisible(), true);
});

await step('Play anyway starts the game, saying it was the player\'s choice', async () => {
  await page.locator('#play-anyway').click();
  await page.waitForFunction(() => window.calls.filter(c => c.cmd === 'launch').length === 2, null, { timeout: 5000 });
  const sent = await launches();
  assert.equal(sent[1].playAnyway, true);
  await page.waitForFunction(() => !document.querySelector('.play-controller-setup'), null, { timeout: 5000 });
});

await step('the choice is not remembered: the next Play asks first again', async () => {
  await page.locator('#play-now').click();
  await page.waitForFunction(() => window.calls.filter(c => c.cmd === 'launch').length === 3, null, { timeout: 5000 });
  assert.equal((await launches())[2].playAnyway, false);
});

await step('the page raised no script errors', async () => assert.deepEqual(errors, []));

await browser.close(); server.close();
if (failures.length) { console.log(`FAIL: ${failures.length} of 6`); process.exit(1); }
console.log('PASS: Play asks first, says the controller needs setting up without calling it a failure, offers both ways forward, and never remembers "play anyway".');
