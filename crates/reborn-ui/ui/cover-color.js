// ui/cover-color.js  (a verbatim copy of the backend's src/web/cover-color.js; keep the two in step)
// The two colours a profile cover is drawn from, read from the avatar's pixels (RGBA). Colourful pixels outweigh grey
// ones; transparent and near-black pixels are skipped. The second colour is the most common one clearly different from
// the first, else a darker shade of the first. Null when nothing usable is left (the page then keeps its neutral cover).
const hex = (c) => `#${c.map((v) => Math.max(0, Math.min(255, Math.round(v))).toString(16).padStart(2, "0")).join("")}`;
const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);

export function dominantColors(data, step = 4) {
  const bins = new Map();
  for (let i = 0; i + 3 < (data?.length ?? 0); i += 4 * step) {
    if (data[i + 3] < 128) continue;
    const r = data[i], g = data[i + 1], b = data[i + 2];
    const max = Math.max(r, g, b), min = Math.min(r, g, b);
    if (max < 24) continue;
    const key = ((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4);
    const w = 1 + (max - min) / 64;
    const bin = bins.get(key) ?? { n: 0, r: 0, g: 0, b: 0 };
    bin.n += w; bin.r += r * w; bin.g += g * w; bin.b += b * w;
    bins.set(key, bin);
  }
  const ranked = [...bins.values()].sort((a, b) => b.n - a.n).map((x) => [x.r / x.n, x.g / x.n, x.b / x.n]);
  if (!ranked.length) return null;
  const first = ranked[0];
  const second = ranked.find((c) => dist(c, first) > 96) ?? first.map((v) => v * 0.55);
  return [hex(first), hex(second)];
}
