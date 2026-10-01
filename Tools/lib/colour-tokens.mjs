// Old colours onto the theme's tokens (spec §3.1), for the layout files Tools/extract-layout-css.mjs writes.
// Input: a computed colour [r, g, b, a] (0-255, alpha 0-1). Output: a CSS value that names only theme tokens.
export const TEXT = { '--fg': [244, 245, 247], '--fg-2': [174, 181, 192], '--fg-3': [124, 132, 145], '--red-ink': [255, 122, 127], '--ok': [61, 220, 151], '--warn': [255, 196, 107] };
const STATE = { red: '--red', amber: '--warn', green: '--ok' };
const dist = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
// How light a colour looks over the near-black window (#0d0e11 is about .05).
const light = ([r, g, b, a]) => a * ((0.2126 * r + 0.7152 * g + 0.0722 * b) / 255) + (1 - a) * 0.05;
const tint = (token, a, lo, hi) => `color-mix(in srgb,var(${token}) ${Math.max(lo, Math.min(hi, Math.round(a * 100)))}%,transparent)`;

export function hue([r, g, b]) {
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  if (max < 60 || (max - min) / max < 0.35) return 'neutral';
  if (r === max && g < r * 0.6 && b < r * 0.7) return 'red';
  if (r === max && g >= r * 0.6 && b < g * 0.75) return 'amber';
  return 'green';   // green, teal and blue: the theme's one cool state colour is --ok
}

/** Text, icons (fill/stroke), carets: the nearest of the six text tokens. */
export function textToken(c) {
  if (c[3] < 0.02) return 'transparent';
  let best = '--fg', d = Infinity;
  for (const [token, v] of Object.entries(TEXT)) { const x = dist(c, v); if (x < d) { d = x; best = token; } }
  return `var(${best})`;
}

/** Backgrounds: a state colour stays one (solid, or a tint when it was faint); anything neutral becomes a glass level. */
export function fillToken(c) {
  if (c[3] < 0.02) return 'transparent';
  const h = hue(c);
  if (h !== 'neutral') return c[3] >= 0.6 ? `var(${STATE[h]})` : tint(STATE[h], c[3], 6, 40);
  const e = light(c);
  if (e > 0.5) return 'var(--fg)';
  return e < 0.107 ? 'var(--glass-1)' : e < 0.138 ? 'var(--glass-2)' : 'var(--glass-3)';
}

/** Borders and rules: --stroke or --stroke-hi; a state colour keeps its colour as a tint. */
export function lineToken(c) {
  if (c[3] < 0.02) return 'transparent';
  const h = hue(c);
  if (h !== 'neutral') return tint(STATE[h], Math.max(c[3], 0.35), 35, 100);
  return light(c) >= 0.169 ? 'var(--stroke-hi)' : 'var(--stroke)';
}
