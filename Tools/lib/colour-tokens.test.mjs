import { test } from 'node:test';
import assert from 'node:assert/strict';
import { hue, textToken, fillToken, lineToken } from './colour-tokens.mjs';

test('hue', () => {
  assert.equal(hue([224, 39, 46]), 'red');
  assert.equal(hue([243, 195, 106]), 'amber');
  assert.equal(hue([34, 199, 180]), 'green');
  assert.equal(hue([128, 128, 128]), 'neutral');
  assert.equal(hue([19, 21, 25]), 'neutral');
});

test('text colours go to the nearest text token', () => {
  assert.equal(textToken([255, 255, 255, 1]), 'var(--fg)');
  assert.equal(textToken([164, 172, 184, 1]), 'var(--fg-2)');
  assert.equal(textToken([133, 135, 130, 1]), 'var(--fg-3)');
  assert.equal(textToken([34, 199, 180, 1]), 'var(--ok)');
  assert.equal(textToken([243, 195, 106, 1]), 'var(--warn)');
  assert.equal(textToken([255, 154, 154, 1]), 'var(--red-ink)');
  assert.equal(textToken([0, 0, 0, 0]), 'transparent');
});

test('fills: state colours stay state colours, neutral fills become glass', () => {
  assert.equal(fillToken([224, 39, 46, 1]), 'var(--red)');
  assert.equal(fillToken([224, 39, 46, 0.1]), 'color-mix(in srgb,var(--red) 10%,transparent)');
  assert.equal(fillToken([34, 199, 180, 0.12]), 'color-mix(in srgb,var(--ok) 12%,transparent)');
  assert.equal(fillToken([243, 195, 106, 1]), 'var(--warn)');
  assert.equal(fillToken([19, 21, 25, 1]), 'var(--glass-1)');
  assert.equal(fillToken([255, 255, 255, 0.045]), 'var(--glass-1)');
  assert.equal(fillToken([255, 255, 255, 0.075]), 'var(--glass-2)');
  assert.equal(fillToken([255, 255, 255, 0.11]), 'var(--glass-3)');
  assert.equal(fillToken([255, 255, 255, 1]), 'var(--fg)');
  assert.equal(fillToken([0, 0, 0, 0]), 'transparent');
});

test('lines: neutral lines become strokes, state lines a tint', () => {
  assert.equal(lineToken([255, 255, 255, 0.09]), 'var(--stroke)');
  assert.equal(lineToken([255, 255, 255, 0.12]), 'var(--stroke)');
  assert.equal(lineToken([255, 255, 255, 0.16]), 'var(--stroke-hi)');
  assert.equal(lineToken([224, 39, 46, 0.25]), 'color-mix(in srgb,var(--red) 35%,transparent)');
  assert.equal(lineToken([0, 0, 0, 0]), 'transparent');
});
