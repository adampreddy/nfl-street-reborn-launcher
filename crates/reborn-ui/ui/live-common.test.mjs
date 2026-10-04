import { test } from 'node:test';
import assert from 'node:assert/strict';
import { assetUrl, LIVE_ORIGIN } from './live-common.js';

test('a portrait or logo path is kept, root-relative or already under the backend origin; anything else is refused', () => {
  const p = '/live-assets/r/2026.09.23-seq-7.bb022c2854/portraits/1-96.png';
  assert.equal(assetUrl(p), LIVE_ORIGIN + p);
  assert.equal(assetUrl(LIVE_ORIGIN + p), LIVE_ORIGIN + p, 'the absolute spelling of the same image is not dropped');
  assert.equal(assetUrl('/live-assets/r/v1/portraits/pickup-hunter-96.png'), `${LIVE_ORIGIN}/live-assets/r/v1/portraits/pickup-hunter-96.png`, 'any catalog file name');
  for (const bad of ['https://evil.example/live-assets/x.png', `${LIVE_ORIGIN}.evil.example/live-assets/x.png`, '/live-assets/../x.png',
    '/other/x.png', '/live-assets/x.jpg', null, undefined, 7, '']) assert.equal(assetUrl(bad), null, String(bad));
});
