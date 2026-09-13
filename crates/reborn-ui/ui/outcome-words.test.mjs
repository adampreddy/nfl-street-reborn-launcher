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
