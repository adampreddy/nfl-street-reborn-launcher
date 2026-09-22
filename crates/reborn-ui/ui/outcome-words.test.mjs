import { test } from 'node:test';
import assert from 'node:assert/strict';

// onboarding.js is a browser page: merely importing it runs module-level
// code that wires up `document.getElementById('next')` and friends as a
// side effect (see Tools/check-shell-pages.mjs, which fakes the same DOM
// for the other setup screens). Node has no DOM, so a minimal fake one is
// installed before the module loads. A dynamic import is required rather
// than a static one because static imports are evaluated before any of
// this file's own top-level code runs, which would defeat the shim.
function fakeEl() {
  return {
    className: '', textContent: '', attrs: {},
    append() {}, replaceChildren() {}, addEventListener() {},
    setAttribute(k, v) { this.attrs[k] = v; },
    querySelector() { return null; }, querySelectorAll() { return []; },
  };
}
global.window = {};
global.document = {
  getElementById: () => fakeEl(),
  createElement: () => fakeEl(),
  querySelectorAll: () => [],
  addEventListener() {},
};

const { outcomeWords } = await import('./onboarding.js');

test('an unrecognized NFL Street 2 disc must not be reported as success', () => {
  const [headline] = outcomeWords('unknown', '', '', false);
  assert.ok(
    !headline.includes('!'),
    `an unconverted disc reported success: ${headline}`
  );
});

test('a missing patch chain must not be reported as success', () => {
  const [headline] = outcomeWords('no-chain', '', '', false);
  assert.ok(
    !headline.includes('!'),
    `a missing chain reported success: ${headline}`
  );
});

// A disc Reborn recognized, whose patch step refused or could not run, is a
// patch failure and not a verdict on the disc. Passed through as 'unknown' or
// 'no-chain' it rendered "Your game file isn't NFL Street Reborn" over a
// build Reborn had just verified, with Continue enabled and the Play badge
// silent: three surfaces disagreeing about one file.
test('a verified Reborn build whose update did not run is not accused of being someone else\'s disc', () => {
  const [headline, detail] = outcomeWords('not-updated', '', 'The update could not be applied right now.', false);
  assert.ok(
    !headline.includes("isn't NFL Street Reborn"),
    `a verified Reborn build was told it is not one: ${headline}`
  );
  assert.ok(
    detail.includes('NFL Street Reborn build'),
    `the words never say the disc is fine: ${detail}`
  );
  assert.ok(
    detail.includes('could not be applied'),
    `the words never say what actually failed: ${detail}`
  );
});

// Recovery runs by itself when a chosen disc will not convert, so the only
// place the player learns their own file was left alone is this note. The
// 'downloaded' case used a fixed sentence and threw the note away.
test('a disc that could not be converted is told what setup did instead', () => {
  const note = 'Reborn could not convert the game file you chose, so it downloaded and verified the Reborn game for you. Your own file was not changed.';
  const [headline, detail] = outcomeWords('downloaded', '', note, false);
  assert.ok(headline.includes('ready'), `recovery did not report success: ${headline}`);
  assert.ok(
    detail.includes('Your own file was not changed'),
    `the player is never told their disc was left alone: ${detail}`
  );
});

test('a download nobody had to be rescued from still reads as a plain success', () => {
  const [, detail] = outcomeWords('downloaded', '', '', false);
  assert.ok(
    detail.includes('downloaded and verified'),
    `the plain download lost its words: ${detail}`
  );
  assert.ok(
    !detail.includes('could not convert'),
    `a plain download blames a disc nobody chose: ${detail}`
  );
});
