// Shared colour helpers for a team's own primary colour, used by both the native match view
// (live.js) and "Live now" on the Online page (live-now.js) so a team's colour never has to be
// duplicated as raw CSS: it is only ever read as a validated #rrggbb and used as a chip's
// background, with black or white text picked here so the chip clears WCAG 4.5:1 contrast.

/** WCAG relative luminance of a validated #rrggbb. */
export function luminance(hex) {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}

/** Black or white text on `hex`, whichever contrasts more (the better of the two is always 4.58:1 or more). */
export function inkFor(hex) {
  const l = luminance(hex);
  return (l + 0.05) / 0.05 >= 1.05 / (l + 0.05) ? '#000000' : '#ffffff';
}
