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

test('discardAll reverts newest first and never saves', async () => {
  const d = createDraft(), order = [];
  d.put('a', { label: 'Window glass', value: false, save: async () => { throw new Error('no save on discard'); }, revert: async () => { order.push('a'); } });
  d.put('b', { label: 'Pause', value: 'true', save: async () => { throw new Error('no save on discard'); } });
  d.put('c', { label: 'Slow animations', value: false, save: async () => { throw new Error('no save on discard'); }, revert: async () => { order.push('c'); } });
  await d.discardAll();
  assert.deepEqual(order, ['c', 'a']);
  assert.deepEqual(d.changes(), []);
});

test('a revert that fails throws its reason and keeps it and everything not yet reverted', async () => {
  const d = createDraft(), order = [];
  d.put('a', { label: 'Window glass', value: false, save: noop, revert: async () => { order.push('a'); } });
  d.put('b', { label: 'Pause', value: 'true', save: noop });
  d.put('c', { label: 'Slow animations', value: false, save: noop, revert: async () => { order.push('c'); throw new Error('could not'); } });
  d.put('e', { label: 'Emulator', value: 'x.exe', save: noop });
  await assert.rejects(d.discardAll(), { message: 'Slow animations: could not' });
  assert.deepEqual(order, ['c'], 'nothing older is reverted after a failure');
  assert.deepEqual(d.changes(), ['Window glass', 'Pause', 'Slow animations'], 'the failed change and the older ones stay pending');
  assert.equal(d.value('c', true), false);
});

test('a change made while its save is in flight survives the save', async () => {
  const d = createDraft();
  let release;
  d.put('a', { label: 'Alpha', value: 1, save: () => new Promise((r) => { release = r; }) });
  const saving = d.saveAll();
  d.put('a', { label: 'Alpha', value: 2, save: noop });
  release();
  assert.deepEqual(await saving, { saved: 1, stopped: null });
  assert.deepEqual(d.changes(), ['Alpha']);
  assert.equal(d.value('a', 0), 2);
});

test('overlapping saveAll calls run the saves once', async () => {
  const d = createDraft();
  let calls = 0, release;
  d.put('a', { label: 'Alpha', value: 1, save: () => { calls++; return new Promise((r) => { release = r; }); } });
  const first = d.saveAll(), second = d.saveAll();
  release();
  await Promise.all([first, second]);
  assert.equal(calls, 1);
  assert.deepEqual(d.changes(), []);
});

test('a discard pressed during a save waits for it, then reverts only what is still pending', async () => {
  const d = createDraft(), order = [];
  let release;
  d.put('a', { label: 'Alpha', value: 1, save: () => new Promise((r) => { release = () => { order.push('saved a'); r(); }; }), revert: async () => { order.push('revert a'); } });
  const saving = d.saveAll();
  const discarding = d.discardAll();
  release();
  await saving;
  assert.equal(await discarding, undefined);
  assert.deepEqual(order, ['saved a']);
  assert.deepEqual(d.changes(), []);
});

test('overlapping discardAll calls revert once', async () => {
  const d = createDraft();
  let reverts = 0;
  d.put('a', { label: 'Alpha', value: 1, save: noop, revert: async () => { reverts++; await new Promise((r) => setTimeout(r, 5)); } });
  await Promise.all([d.discardAll(), d.discardAll()]);
  assert.equal(reverts, 1);
});

test('a reason is never "{}"', async () => {
  const d = createDraft();
  d.put('a', { label: 'Alpha', value: 1, save: async () => { throw {}; } });
  const { stopped } = await d.saveAll();
  assert.notEqual(stopped.reason, '{}');
  assert.match(stopped.reason, /^[A-Z][^{}]*\.$/);
  d.put('b', { label: 'Beta', value: 1, save: noop, revert: async () => { throw new Error(''); } });
  await assert.rejects(d.discardAll(), (e) => !/\{\}|: $/.test(e.message));
});
