// Every controller can be set up by pressing its buttons, and a controller that
// needs it is TOLD so.
//
// Reports, 2026-09-18: "test shows its working but when i open the app no
// connection" (PlayStation 5), "it won't read the stick inputs, only the buttons
// work" (USB GameCube). The workspace players actually use read a press through
// the BROWSER, saved Xbox names, and for every device that was not XInput or SDL
// asked for "the input name from Dolphin", which no player ever sees. Every
// PlayStation pad and every generic pad is a WGInput device. And a pad whose
// controls Windows cannot name was told its "mapping is ready".
//
// Ablation: SETUP_CONTROLLER_JS=<the file before this change> must FAIL.
import { chromium } from 'playwright';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';

const root = path.resolve(import.meta.dirname, '..');
const script = process.env.SETUP_CONTROLLER_JS || path.join(root, 'crates/reborn-ui/ui/setup-controller.js');
const source = readFileSync(script, 'utf8');

const browser = await chromium.launch({ channel: 'msedge', headless: true });
const failures = [];
const check = async (name, body) => {
  const page = await browser.newPage();
  try {
    await page.goto('about:blank');
    await body(page);
    console.log('ok    ' + name);
  } catch (e) {
    failures.push(name);
    console.log('FAIL  ' + name + '\n      ' + String(e.message || e).split('\n')[0]);
  } finally {
    await page.close();
  }
};

// Mount the real workspace over a backend that answers like the launcher does.
const mount = (page, { device, rows, presses }) =>
  page.evaluate(
    ({ source, device, rows, presses }) => {
      window.calls = [];
      const saved = {};
      const queue = [...presses];
      window.__TAURI__ = {
        core: {
          invoke: async (cmd, args = {}) => {
            window.calls.push({ cmd, args: JSON.parse(JSON.stringify(args)) });
            if (cmd === 'setup_controller_mapping')
              return { device, custom: false, checked: true, rows: rows.map(r => ({ key: r.key, label: r.label, binding: saved[r.key] || r.binding, dead: r.dead && !saved[r.key] })) };
            if (cmd === 'remap_start') return { pads: [] };
            if (cmd === 'remap_detect') return { binding: queue.length ? queue.shift() : null };
            if (cmd === 'setup_save_mapping') { Object.assign(saved, args.bindings); return; }
            return;
          },
        },
      };
      new Function(source)();
      const root = window.createSetupController({ device, runtime: 'C:/runtime/Dolphin.exe', onBusy() {}, layout: 'players', singleDevice: true });
      document.body.append(root);
    },
    { source, device, rows, presses },
  );

const named = cmd => page => page.evaluate(cmd => window.calls.filter(c => c.cmd === cmd), cmd);
const settle = page => page.waitForTimeout(150);

await check('a pad with numbered controls is told it needs setting up, and how many controls', async page => {
  await mount(page, {
    device: 'WGInput/0/Generic USB Joystick',
    presses: [],
    rows: [
      { key: 'Buttons/A', label: 'A', binding: '`Button A`|`Cross`|`Button S`', dead: true },
      { key: 'Buttons/B', label: 'B', binding: '`Button B`|`Circle`|`Button E`', dead: true },
      { key: 'Main Stick/Up', label: 'Up', binding: '`Left Y+`', dead: true },
      { key: 'D-Pad/Up', label: 'Up', binding: '`Pad N`|`Switch 0 N`', dead: false },
    ],
  });
  await settle(page);
  const notice = page.locator('.controller-needs-setup');
  assert.equal(await notice.count(), 1, 'there is no notice element at all');
  assert.equal(await notice.isVisible(), true, 'the notice is hidden');
  assert.match(await notice.innerText(), /3 of its 4 controls/);
  assert.doesNotMatch(await page.locator('.mapping-status').innerText(), /mapped automatically|is ready/i, 'it still claims the mapping is ready');
});

await check('setting it up reads each press from the runtime and saves only what was pressed', async page => {
  await mount(page, {
    device: 'WGInput/0/Generic USB Joystick',
    // A is pressed, B is skipped by waiting, the stick is pushed up.
    presses: ['`Button 2`', null, '`Axis 1-`'],
    rows: [
      { key: 'Buttons/A', label: 'A', binding: '`Button A`|`Cross`|`Button S`', dead: true },
      { key: 'Buttons/B', label: 'B', binding: '`Button B`|`Circle`|`Button E`', dead: true },
      { key: 'Main Stick/Up', label: 'Up', binding: '`Left Y+`', dead: true },
      { key: 'D-Pad/Up', label: 'Up', binding: '`Pad N`|`Switch 0 N`', dead: false },
    ],
  });
  await settle(page);
  await page.locator('.controller-needs-setup button').click();
  await page.waitForFunction(() => window.calls.some(c => c.cmd === 'setup_save_mapping'), null, { timeout: 5000 });
  await settle(page);

  const starts = await named('remap_start')(page);
  assert.equal(starts.length, 1, 'one listener for the whole pass');
  assert.deepEqual(starts[0].args, { runtime: 'C:/runtime/Dolphin.exe' });
  const detects = await named('remap_detect')(page);
  assert.equal(detects.length, 3, 'one press asked for per control that cannot be pressed');
  assert.deepEqual(detects[0].args, { device: 'WGInput/0/Generic USB Joystick' });
  const saves = await named('setup_save_mapping')(page);
  assert.equal(saves.length, 1, 'saved together, once');
  assert.deepEqual(saves[0].args.bindings, { 'Buttons/A': '`Button 2`', 'Main Stick/Up': '`Axis 1-`' }, 'a skipped control is not saved as anything');

  const order = await page.evaluate(() => window.calls.map(c => c.cmd));
  assert.ok(order.lastIndexOf('remap_stop') > order.lastIndexOf('remap_detect'), 'the listener is left holding the pad open');
  assert.match(await page.locator('.controller-needs-setup').innerText(), /1 of its 4 controls/, 'the control that was skipped is still reported');
});

await check('a PlayStation pad is remapped by pressing a button, never by typing a Dolphin name', async page => {
  await mount(page, {
    device: 'WGInput/0/PS5 Controller',
    presses: ['`Circle`'],
    rows: [
      { key: 'Buttons/A', label: 'A', binding: '`Cross`', dead: false },
      { key: 'Buttons/B', label: 'B', binding: '`Circle`', dead: false },
    ],
  });
  await settle(page);
  assert.equal(await page.locator('.controller-needs-setup').isVisible().catch(() => false), false, 'a pad that is fully mapped is not nagged');
  await page.getByRole('button', { name: 'Assign A', exact: true }).click();
  await page.waitForFunction(() => window.calls.some(c => c.cmd === 'setup_save_mapping'), null, { timeout: 5000 });
  const saves = await named('setup_save_mapping')(page);
  assert.deepEqual(saves[0].args.bindings, { 'Buttons/A': '`Circle`' }, 'what the RUNTIME heard is what is saved');
  assert.doesNotMatch(await page.locator('body').innerText(), /input name from Dolphin for this controller/i);
  const order = await page.evaluate(() => window.calls.map(c => c.cmd));
  assert.ok(order.includes('remap_stop'), 'the listener is stopped after a single assignment');
});

await browser.close();
if (failures.length) {
  console.log(`FAIL: ${failures.length} of 3`);
  process.exit(1);
}
console.log('PASS: a pad that needs setting up is told so, any pad is mapped by pressing, and the listener never outlives the page.');
