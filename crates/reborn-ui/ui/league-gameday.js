// League game day (spec docs/superpowers/specs/2026-10-21-league-game-day-design.md, mockup gameday-v4): your league
// fixture's room on the League tab. Both lineups with their portraits and season lines, the tale of the tape between
// them, then the launcher's own room card and chat (room-card.js); once the series starts, its state in their place.
// Render only: league-gameday-model.js decides what shows.
import { el } from './shell.js';
import { teamLogo } from './match-rows.js';
import { heldNotice } from './online-model.js';
import { drawRoomCard } from './room-card.js';

// Only a plain hex colour reaches CSS (the site's team colour), never anything else the answer carries.
const tc = (c) => (/^#[0-9a-f]{6}$/i.test(String(c ?? '')) ? `--tc:${c}` : null);

function side(t, { right, logoFor, portraitFor }) {
  const logo = teamLogo({ name: t.name, logo_url: logoFor(t.id) ?? t.logoUrl }, 'gd-logo');
  const rows = t.lineup.map((p) => {
    const face = portraitFor(p.poid) ?? (p.portraitUrl ? null : logoFor(t.id));
    return el('div', { class: 'gd-pl' },
      face ? el('img', { class: 'gd-face', src: face, alt: '' }) : el('span', { class: 'gd-face', 'aria-hidden': 'true' }),
      el('div', { class: 'gd-who' }, el('b', { text: p.name }), el('span', { class: 'num', text: [p.pos, p.number === null ? null : `#${p.number}`].filter(Boolean).join(' · ') })),
      el('span', { class: `gd-ln num${p.line === 'No stats kept' ? ' none' : ''}`, text: p.line }));
  });
  return el('section', { class: `glass gd-side${right ? ' r' : ''}`, style: tc(t.color), 'aria-label': t.name },
    el('div', { class: 'gd-sh' }, logo, el('div', {}, el('h3', { text: t.name }), el('p', { class: 'num', text: [t.owner, t.record].filter(Boolean).join(' · ') }))),
    ...(rows.length ? rows : [el('p', { class: 'gd-none', text: 'Lineup not available' })]));
}

function tapeCard(g, { logoFor }) {
  const vs = el('div', { class: 'gd-vs' },
    el('span', { class: 'l' }, teamLogo({ name: g.me.name, logo_url: logoFor(g.me.id) ?? g.me.logoUrl }, 'gd-vs-logo'), el('b', { class: 'num', text: g.me.record })),
    el('i', { text: 'VS' }),
    el('span', { class: 'r' }, el('b', { class: 'num', text: g.opp?.record ?? '' }), g.opp ? teamLogo({ name: g.opp.name, logo_url: logoFor(g.opp.id) ?? g.opp.logoUrl }, 'gd-vs-logo') : null));
  const headRow = el('div', { class: 'gd-tape-h' }, el('h2', { text: 'Tale of the tape' }), g.tape ? el('span', { text: 'Season' }) : null);
  if (!g.tape) return el('section', { class: 'glass gd-tape', 'aria-label': 'Tale of the tape' }, headRow, vs, el('p', { class: 'gd-empty', text: 'No games yet' }));
  const bar = (w, color, better) => el('i', { style: `--w:${w}%${color ? `;--c:${color}` : ''};--o:${better ? 1 : 0.4}` });
  const cA = /^#[0-9a-f]{6}$/i.test(g.me.color) ? g.me.color : null;
  const cB = /^#[0-9a-f]{6}$/i.test(g.opp?.color ?? '') ? g.opp.color : null;
  return el('section', { class: 'glass gd-tape', 'aria-label': 'Tale of the tape' }, headRow, vs,
    ...g.tape.map((r) => el('div', { class: 'gd-st' },
      el('div', { class: 'gd-st-lab' }, el('span', { class: `num${r.mineBetter ? ' w' : ''}`, text: r.mine.toLocaleString('en-US') }), r.label,
        el('span', { class: `num${r.theirsBetter ? ' w' : ''}`, text: r.theirs.toLocaleString('en-US') })),
      el('div', { class: 'gd-bars', 'aria-hidden': 'true' }, bar(r.mineWidth, cA, r.mineBetter), bar(r.theirsWidth, cB, r.theirsBetter)))));
}

const SERIES_WORDS = (m) => {
  if (m.relay_state === 'complete') return 'Final';
  if (m.relay_state === 'running' || m.relay_state === 'intermission') {
    const s = Array.isArray(m.series) ? m.series : [0, 0];
    return `Game ${Number(s[0] || 0) + Number(s[1] || 0) + 1} of 3`;
  }
  return 'Starting…';
};

function matchCard(m, { mmAct, onBack }) {
  const s = Array.isArray(m.series) ? m.series : [0, 0];
  const held = m.relay_state === 'held' ? heldNotice(m) : null;
  const done = m.relay_state === 'complete' || held;
  const btns = [];
  if (m.relay_state === 'pairing' && !m.your_accepted) {
    btns.push(el('button', { type: 'button', class: 'btn primary', 'data-key': 'gd-accept', 'data-primary': 'true', text: 'Accept',
      onclick: () => mmAct('mm_match_action', { matchId: m.id, action: 'accept', value: true }) }));
  }
  if (done) btns.push(el('button', { type: 'button', class: 'btn primary', 'data-key': 'gd-back', 'data-primary': 'true', text: 'Back to League', onclick: onBack }));
  else btns.push(el('button', { type: 'button', class: 'btn quiet', 'data-key': 'gd-leave-match', text: 'Leave match',
    onclick: () => mmAct('mm_match_action', { matchId: m.id, action: 'leave', value: null }) }));
  return el('section', { class: 'glass gd-match', role: 'status', 'aria-label': 'Your series' },
    el('div', { class: 'gd-m-score num', text: `${s[0] ?? 0}-${s[1] ?? 0}` }),
    el('div', { class: 'gd-m-text' }, el('b', { text: held ? held.title : SERIES_WORDS(m) }), held ? el('p', { text: held.text }) : null),
    el('div', { class: 'gd-m-btns' }, ...btns));
}

/** The game-day view's root. `g` is gameDay()'s model; `chat` the snapshot's chat lines. */
export function drawGameDay(g, chat, { mmAct, logoFor, portraitFor, onBack, staleLine }) {
  const bottom = g.match
    ? [matchCard(g.match, { mmAct, onBack })]
    : g.room ? drawRoomCard(g.room, chat, { act: mmAct, title: 'Your room', sub: 'League rules · Best of 3', holdUp: false }) : [];
  return el('div', { class: 'gd' },
    el('div', { class: 'gd-top' },
      el('button', { type: 'button', class: 'gd-back', 'data-key': 'gd-league', text: '‹ League', onclick: onBack }),
      // Old league data says so in the top line (one screen at 1280x800, no extra row).
      g.stale && staleLine ? el('span', { class: 'gd-stale', role: 'status', text: staleLine }) : null,
      el('span', { class: 'num', text: g.title })),
    el('div', { class: 'gd-board' },
      side(g.me, { right: false, logoFor, portraitFor }),
      tapeCard(g, { logoFor }),
      g.opp ? side(g.opp, { right: true, logoFor, portraitFor }) : el('section', { class: 'glass gd-side r' }, el('p', { class: 'gd-none', text: 'Lineup not available' }))),
    el('div', { class: `gd-room${g.match ? ' match' : ''}` }, ...bottom));
}
