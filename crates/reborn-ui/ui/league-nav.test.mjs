import { test } from 'node:test';
import assert from 'node:assert/strict';

// shell.js runs a startup block only when `window.__TAURI__` exists; the test has none, so importing it is side-effect
// free apart from any look.js import, which fails quietly (caught) outside a page.
globalThis.window = globalThis.window || {};
const { addLeagueLink } = await import('./shell.js');

function fakeDoc(hrefs) {
  const links = hrefs.map((href) => ({ href, getAttribute: (k) => (k === 'href' ? href : null) }));
  const inserted = [];
  const nav = {
    querySelector: (sel) => {
      const m = sel.match(/a\[href="([^"]+)"\]/);
      const hit = m && links.find((l) => l.href === m[1]);
      if (!hit) return null;
      return { ...hit, insertAdjacentElement: (where, node) => { inserted.push([where, hit.href, node]); } };
    },
  };
  const doc = {
    querySelector: (sel) => (sel === '.shell-nav' ? nav : null),
    createElement: (tag) => ({ tag, attrs: {}, children: [], setAttribute(k, v) { this.attrs[k] = v; }, append(...c) { this.children.push(...c); }, set innerHTML(_) { throw new Error('no innerHTML'); } }),
    createElementNS: (_ns, tag) => ({ tag, attrs: {}, children: [], setAttribute(k, v) { this.attrs[k] = v; }, append(...c) { this.children.push(...c); } }),
  };
  return { doc, inserted };
}

test('adds League right after Online', () => {
  const { doc, inserted } = fakeDoc(['index.html', 'online.html', 'stats.html']);
  assert.equal(addLeagueLink(doc), true);
  assert.equal(inserted.length, 1);
  assert.deepEqual([inserted[0][0], inserted[0][1], inserted[0][2].attrs.href], ['afterend', 'online.html', 'league.html']);
});

test('never twice, and not on a page without the sidebar', () => {
  assert.equal(addLeagueLink(fakeDoc(['online.html', 'league.html']).doc), false);
  assert.equal(addLeagueLink(fakeDoc(['index.html']).doc), false, 'no Online link: onboarding and harness pages');
  assert.equal(addLeagueLink({ querySelector: () => null }), false);
});
