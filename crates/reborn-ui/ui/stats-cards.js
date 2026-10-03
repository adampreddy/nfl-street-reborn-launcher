// The v2 cards and charts Stats and Profile share (owner 10-03, the approved mv2 mockup): a card with its 48px header,
// player rows with five numbers, the W/L strip, share bars and the charts. Each chart draws its shapes in an SVG
// stretched to its box (strokes do not scale) and every word and dot in HTML, so nothing stretches at any width.
// Server text only through el()'s `text`; images only through assetUrl() (the CSP's image prefixes). The only
// script-set styles are custom properties (--x, --y, --w, --n) through the CSSOM.
import { el } from './shell.js';
import { assetUrl } from './live-common.js';
import { statCells, teamColour } from './stats-model.js';

const SVG_NS = 'http://www.w3.org/2000/svg';
let uid = 0;

export function svg(tag, attrs = {}) {
  const node = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) node.setAttribute(k, String(v));
  return node;
}
const canSvg = () => typeof document !== 'undefined' && typeof document.createElementNS === 'function';
/** Set custom properties through the CSSOM (the fake DOM of the shell check has no style). */
export function vars(node, map) {
  if (node?.style?.setProperty) for (const [k, v] of Object.entries(map)) node.style.setProperty(k, String(v));
  return node;
}

/**
 * A v2 card: `<section class="glass vc s<span>">` with its header (title; `aside` text or a node on the right) and a
 * body. `flush`: the body nodes go straight in (tables, cell grids); else into a padded `.vc-b`.
 */
export function card(id, title, body, { aside = null, flush = false, span = 12, cls = '' } = {}) {
  const right = typeof aside === 'string' ? el('span', { class: 'vc-aside', text: aside }) : aside;
  const kids = (Array.isArray(body) ? body : [body]).filter(Boolean);
  return el('section', { id, class: `glass vc s${span}${cls ? ` ${cls}` : ''}`, 'aria-labelledby': `${id}-h` },
    el('div', { class: 'vc-h' }, el('h2', { id: `${id}-h`, text: title }), right), ...(flush ? kids : [el('div', { class: 'vc-b' }, ...kids)]));
}

/** A table header row (hidden from screen readers: each row carries its own label). `cols` is a grid-template-columns. */
export function tableHead(labels, cols, right = []) {
  return vars(el('div', { class: 'vc-th', 'aria-hidden': 'true' }, ...labels.map((t, i) => el('span', { class: right.includes(i) ? 'r' : null, text: t }))), { 'grid-template-columns': cols });
}
/** Grid columns go on as a property, never a style attribute. */
export function cols(node, template) {
  if (node?.style) node.style.gridTemplateColumns = template;
  return node;
}

export function wlStrip(results) {
  return el('div', { class: 'wl-strip', role: 'img', 'aria-label': `Last ${results.length}: ${results.map((r) => (r === 'W' ? 'win' : 'loss')).join(', ')}` },
    ...results.map((r) => el('span', { class: r, 'aria-hidden': 'true', text: r })));
}

export function share(fraction, cls = '') {
  return el('span', { class: `share${cls ? ` ${cls}` : ''}`, 'aria-hidden': 'true' }, vars(el('i'), { '--w': Math.max(0, Math.min(1, fraction || 0)) }));
}

/** A portrait on the team colour with the team badge (falls back to the initial). */
export function face(c) {
  const box = el('span', { class: 'player-face', 'aria-hidden': 'true' });
  const tc = teamColour(c.primary);
  if (tc) vars(box, { '--tc': tc });
  const letter = () => el('span', { class: 'face letter', text: (c.name || '').charAt(0).toUpperCase() });
  const src = assetUrl(c.portrait_url);
  if (src) { const img = el('img', { class: 'face', alt: '', src }); img.addEventListener('error', () => img.replaceWith(letter()), { once: true }); box.append(img); }
  else box.append(letter());
  const badge = assetUrl(c.logo_url);
  if (badge) { const img = el('img', { class: 'badge', alt: '', src: badge }); img.addEventListener('error', () => img.remove(), { once: true }); box.append(img); }
  return box;
}

export const ROLES = [['passer', 'Best passer'], ['rusher', 'Best rusher'], ['receiver', 'Best receiver'], ['dl_lb', 'Best DL/LB'], ['ballhawk', 'Ballhawk'], ['most_played', 'Most played']];

/**
 * The player rows (Your players / Their players): one per role the server names, its five numbers (statCells). `href`
 * (role) => the address a row opens, or null for a plain row.
 */
export function playerRows(best, { roles = ROLES, href = null } = {}) {
  return roles.flatMap(([role, label]) => {
    const p = best?.[role];
    if (!p || typeof p !== 'object' || !p.name) return [];
    const cells = statCells(role, p);
    const words = cells.map((c) => `${c.value} ${c.label}`).join(', ');
    const tx = el('div', { class: 'pl-tx' }, el('div', { class: 'pl-head' }, el('span', { class: 'pl-name', text: p.name }), el('span', { class: 'pl-role', text: label })),
      cells.length ? el('div', { class: 'pl-cells num' }, ...cells.map((c) => el('span', {}, c.value, el('small', { text: c.label })))) : null);
    const go = href ? href(role) : null;
    const attrs = { class: `pl-row${cells.length ? '' : ' none'}`, 'aria-label': `${label}: ${p.name}${words ? `, ${words}` : ''}` };
    return [go ? el('a', { ...attrs, href: go, 'data-key': `card-${role}` }, face(p), tx) : el('div', { ...attrs, role: 'group' }, face(p), tx)];
  });
}

/** Cells of label · number · note (Team per game, Career highlights, Team records). */
export function cellGrid(items, n = items.length) {
  return vars(el('div', { class: `vc-cells${items.length > n ? ' wrap' : ''}` }, ...items.map((c) => el('div', { role: 'group', 'aria-label': `${c.label}: ${c.value}${c.unit ? ` ${c.unit}` : ''}${c.note ? `, ${c.note}` : ''}` },
    el('span', { class: 'k-label', text: c.label }), el('b', {}, c.value, c.unit ? el('small', { text: c.unit }) : null), c.note ? el('span', { class: 'n', text: c.note }) : null))), { '--n': n });
}

const fmt = (v) => v.toLocaleString('en-US');
const r1 = (x) => Math.round(x * 100) / 100;

/**
 * The Elo line (Stats' Elo card, Profile's Elo history and the hero's sparkline). `values` oldest first; `labels` are
 * { at: 0..1, text } under the line; `mini` draws only the line. Null under two points.
 */
export function eloLine(values, { labels = [], mini = false, peak = null, label = 'Elo over time' } = {}) {
  if (!Array.isArray(values) || values.length < 2 || !canSvg()) return null;
  const lo0 = Math.min(...values), hi0 = Math.max(...values);
  const pad = mini ? Math.max(4, (hi0 - lo0) * 0.12) : 15;
  const lo = lo0 - pad, hi = hi0 + pad;
  const X = (i) => (i / (values.length - 1)) * 100, Y = (v) => (1 - (v - lo) / (hi - lo)) * 100;
  const pts = values.map((v, i) => `${r1(X(i))},${r1(Y(v))}`);
  const id = `chg${++uid}`;
  const chart = svg('svg', { viewBox: '0 0 100 100', preserveAspectRatio: 'none', 'aria-hidden': 'true' });
  const grad = svg('linearGradient', { id, x1: 0, y1: 0, x2: 0, y2: 1 });
  grad.append(svg('stop', { offset: 0, class: 'ch-fill-top' }), svg('stop', { offset: 1, class: 'ch-fill-bot' }));
  const defs = svg('defs'); defs.append(grad); chart.append(defs);
  const area = el('div', { class: 'ch-area' });
  const box = el('div', { class: `ch ch-line${mini ? ' ch-mini' : ''}`, role: 'img', 'aria-label': label }, area);
  if (!mini) {
    const step = hi - lo > 400 ? 100 : hi - lo > 160 ? 50 : 25;
    const ys = el('div', { class: 'ch-y', 'aria-hidden': 'true' });
    for (let g = Math.ceil(lo / step) * step; g < hi; g += step) {
      chart.append(svg('line', { x1: 0, x2: 100, y1: r1(Y(g)), y2: r1(Y(g)), class: 'ch-grid' }));
      ys.append(vars(el('span', { text: fmt(g) }), { '--y': Y(g) / 100 }));
    }
    area.append(ys);
  }
  chart.append(svg('path', { d: `M${pts[0]} L${pts.slice(1).join(' L')} L100,100 L0,100Z`, fill: `url(#${id})` }),
    svg('polyline', { points: pts.join(' '), class: 'ch-stroke' }));
  area.prepend(chart);
  if (!mini && peak !== null) {
    const pk = values.lastIndexOf(Math.max(...values));
    const end = X(pk) > 85;
    area.append(vars(el('span', { class: 'ch-dot peak', 'aria-hidden': 'true' }), { '--x': X(pk) / 100, '--y': Y(values[pk]) / 100 }),
      vars(el('span', { class: `ch-peak${end ? ' end' : ''}`, text: `Peak ${fmt(peak)}` }), { '--x': X(pk) / 100, '--y': Y(values[pk]) / 100 }));
  }
  area.append(vars(el('span', { class: 'ch-dot', 'aria-hidden': 'true' }), { '--x': 1, '--y': Y(values.at(-1)) / 100 }));
  if (!mini && labels.length) box.append(xLabels(labels));
  return box;
}

function xLabels(labels) {
  return el('div', { class: 'ch-x', 'aria-hidden': 'true' }, ...labels.map((l) => vars(el('span', { class: l.at === 0 ? 'first' : l.at === 1 ? 'last' : null, text: l.text }), { '--x': l.at })));
}

/** Points per game: scored (red; darker in a loss) and allowed (grey) side by side per game, oldest first. */
export function pointsBars(series) {
  if (!series || !canSvg()) return null;
  const n = series.games.length, gw = 100 / n, pad = gw * 0.14, bw = (gw - 2 * pad) / 2;
  const Y = (v) => (1 - v / series.max) * 100;
  const chart = svg('svg', { viewBox: '0 0 100 100', preserveAspectRatio: 'none', 'aria-hidden': 'true' });
  const ys = el('div', { class: 'ch-y', 'aria-hidden': 'true' });
  for (let g = 0; g <= series.max; g += series.step) {
    chart.append(svg('line', { x1: 0, x2: 100, y1: r1(Y(g)), y2: r1(Y(g)), class: `ch-grid${g === 0 ? ' base' : ''}` }));
    ys.append(vars(el('span', { text: String(g) }), { '--y': Y(g) / 100 }));
  }
  series.games.forEach((p, i) => {
    const x = i * gw + pad;
    chart.append(svg('rect', { x: r1(x), y: r1(Y(p.for)), width: r1(bw - 0.15), height: r1(100 - Y(p.for)), class: p.won ? 'ch-bar-w' : 'ch-bar-l' }),
      svg('rect', { x: r1(x + bw + 0.15), y: r1(Y(p.against)), width: r1(bw - 0.15), height: r1(100 - Y(p.against)), class: 'ch-bar-a' }));
  });
  const area = el('div', { class: 'ch-area' }, chart, ys);
  const labels = series.games.map((p, i) => (p.label ? { at: (i * gw + gw / 2) / 100, text: p.label } : null)).filter(Boolean);
  const words = series.games.map((p) => `${p.for}-${p.against}`).join(', ');
  return el('div', { class: 'ch ch-bars', role: 'img', 'aria-label': `Points per game, oldest first, scored-allowed: ${words}` }, area,
    el('div', { class: 'ch-x', 'aria-hidden': 'true' }, ...labels.map((l) => vars(el('span', { text: l.text }), { '--x': l.at }))));
}

/** The yards donut: passing (red) and rushing (grey), the total in the hole. */
export function donut(split) {
  if (!split || !canSvg()) return null;
  const s = svg('svg', { viewBox: '0 0 140 140', 'aria-hidden': 'true' });
  const C = 2 * Math.PI * 58, gap = 1.2;
  const a = split.passShare * C;
  s.append(svg('circle', { cx: 70, cy: 70, r: 58, class: 'b', 'stroke-dasharray': `${r1(C - a - gap * 2)} ${r1(a + gap * 2)}`, 'stroke-dashoffset': r1(-(a + gap)) }),
    svg('circle', { cx: 70, cy: 70, r: 58, class: 'a', 'stroke-dasharray': `${r1(a - gap)} ${r1(C - a + gap)}`, 'stroke-dashoffset': 0 }));
  return el('div', { class: 'ch-donut', role: 'img', 'aria-label': `${split.total} yards a game: ${split.pass} passing (${split.passPct}), ${split.rush} rushing (${split.rushPct})` }, s,
    el('div', { class: 'mid', 'aria-hidden': 'true' }, el('b', { text: split.total }), el('small', { text: 'Yards / game' })));
}

/** Results by day: a column per day, a square per match (stacked from the bottom), a date under every fifth day. */
export function dayColumns(byDay) {
  if (!byDay) return null;
  const n = byDay.days.length;
  const grid = vars(el('div', { class: 'ch-days', 'aria-hidden': 'true' }, ...byDay.days.map((d) => el('div', {},
    ...(d.results.length ? d.results.map((r) => el('i', { class: r })) : [el('i')])))), { '--n': n });
  const labels = byDay.days.map((d, i) => (i % 5 === 0 || i === n - 1 ? { at: i === 0 ? 0 : i === n - 1 ? 1 : (i + 0.5) / n, text: d.label } : null)).filter(Boolean);
  const words = byDay.days.filter((d) => d.results.length).map((d) => `${d.label} ${d.results.join('')}`).join(', ');
  return el('div', { class: 'ch', role: 'img', 'aria-label': `Results by day: ${words}` }, grid, xLabels(labels));
}
