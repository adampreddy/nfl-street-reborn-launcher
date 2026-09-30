// The two colours a profile cover is drawn from, read from the avatar's pixels (ui/cover-color.js, a copy of the
// backend's src/web/cover-color.js), and a guard that the profile page never builds HTML from strings.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dominantColors } from "./cover-color.js";

const px = (n, rgba) => Array.from({ length: n }, () => rgba).flat();

test("one colour: that colour and a darker shade of it", () => {
  assert.deepEqual(dominantColors(px(64, [255, 0, 0, 255])), ["#ff0000", "#8c0000"]);
});

test("two distinct colours: the more common first", () => {
  const [a, b] = dominantColors([...px(48, [0, 0, 255, 255]), ...px(16, [255, 255, 0, 255])], 1);
  assert.equal(a, "#0000ff"); assert.equal(b, "#ffff00");
});

test("transparent or near-black only: null", () => {
  assert.equal(dominantColors(px(64, [255, 0, 0, 0])), null);
  assert.equal(dominantColors(px(64, [5, 5, 5, 255])), null);
  assert.equal(dominantColors([]), null);
});

test("profile.js never builds HTML from strings", () => {
  const src = readFileSync(new URL("./profile.js", import.meta.url), "utf8");
  assert.equal(/innerHTML|outerHTML|insertAdjacentHTML|document\.write/.test(src), false);
  // Style attributes are refused by the CSP; the cover's colours go through the CSSOM (style.setProperty) only.
  assert.equal(/setAttribute\(\s*["']style/.test(src), false);
});
