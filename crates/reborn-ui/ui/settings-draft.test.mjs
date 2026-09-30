import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createDraft, HELD } from './settings-draft.js';

const noop = async () => {};

test('changes are listed by label, newest last; undoing one removes it', () => {
  const d = createDraft();
  d.put('a', { label: 'Alpha', value: 1, save: noop });
  d.put('b', { label: 'Beta', value: 2, save: noop });
  d.put('a', { label: 'Alpha', value: 3, save: noop });
  assert.deepEqual(d.changes(), ['Beta', 'Alpha']);
  assert.equal(d.value('a', 0), 3);
  assert.equal(d.value('missing', 9), 9);
  d.put('b', { label: 'Beta', value: 0, same: true, save: noop });
  assert.deepEqual(d.changes(), ['Alpha']);
  assert.equal(d.has('b'), false);
  d.drop('a');
  assert.deepEqual(d.changes(), []);
});

test('saveAll saves in the order the changes were made and empties the list', async () => {
  const d = createDraft(), order = [];
  d.put('a', { label: 'Alpha', value: 1, save: async (v) => { order.push(['a', v]); } });
  d.put('b', { label: 'Beta', value: 2, save: async (v) => { order.push(['b', v]); } });
  assert.deepEqual(await d.saveAll(), { saved: 2, stopped: null });
  assert.deepEqual(order, [['a', 1], ['b', 2]]);
  assert.deepEqual(d.changes(), []);
});

test('saveAll stops at the first failure and keeps it and everything after it', async () => {
  const d = createDraft(), order = [];
  d.put('a', { label: 'Alpha', value: 1, save: async () => { order.push('a'); } });
  d.put('b', { label: 'Beta', value: 2, save: async () => { throw new Error('Dolphin is open'); } });
  d.put('c', { label: 'Gamma', value: 3, save: async () => { order.push('c'); } });
  assert.deepEqual(await d.saveAll(), { saved: 1, stopped: { key: 'b', label: 'Beta', reason: 'Dolphin is open' } });
  assert.deepEqual(order, ['a']);
  assert.deepEqual(d.changes(), ['Beta', 'Gamma']);
});

test('a string rejection is its own reason', async () => {
  const d = createDraft();
  d.put('a', { label: 'Alpha', value: 1, save: async () => { throw 'settings file is locked'; } });
  assert.equal((await d.saveAll()).stopped.reason, 'settings file is locked');
});

test('a held save stops without an error and stays pending', async () => {
  const d = createDraft();
  d.put('g', { label: 'Game image', value: 'x.iso', save: async () => HELD });
  d.put('e', { label: 'Emulator', value: 'y.exe', save: async () => { throw new Error('should not run'); } });
  assert.deepEqual(await d.saveAll(), { saved: 0, stopped: { key: 'g', label: 'Game image', reason: HELD } });
  assert.deepEqual(d.changes(), ['Game image', 'Emulator']);
});

test('discardAll reverts newest first, never saves, and reports revert failures', async () => {
  const d = createDraft(), order = [];
  d.put('a', { label: 'Window glass', value: false, save: async () => { throw new Error('no save on discard'); }, revert: async () => { order.push('a'); } });
  d.put('b', { label: 'Pause', value: 'true', save: async () => { throw new Error('no save on discard'); } });
  d.put('c', { label: 'Slow animations', value: false, save: async () => {}, revert: async () => { order.push('c'); throw new Error('could not'); } });
  assert.deepEqual(await d.discardAll(), ['Slow animations: could not']);
  assert.deepEqual(order, ['c', 'a']);
  assert.deepEqual(d.changes(), []);
});
