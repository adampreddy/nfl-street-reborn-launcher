// The Online page redesign (docs/superpowers/specs/2026-10-02-online-page-redesign-design.md): every mockup state on
// the real page, from the fixed states in lib/page-states.mjs (frozen clock 2026-09-30 20:00 UTC, no network), plus the
// static rules the page's files must keep. Fails on a missing text or control, a broken layout rule, a sideways scroll
// or any page error. Screenshots land in target/online-page.
//     node Tools/check-online-page.mjs
import { chromium } from 'playwright';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { STATES, root, serve, openState } from './lib/page-states.mjs';

const ui = path.join(root, 'crates/reborn-ui/ui');
const out = path.join(root, 'target/online-page');
mkdirSync(out, { recursive: true });
let failures = 0;
const fail = (name, msg) => { failures++; console.log(`FAIL ${name}: ${msg}`); };
const ok = (name) => console.log(`ok   ${name}`);
// A state with no online list still offers the presence switch, under "Discord status", and invents no count.
async function discordStatus(page) {
  const btn = page.locator('#on-players');
  if (!(await btn.isVisible())) return ['the Discord status button is not shown'];
  const p = [];
  if ((await btn.innerText()).trim() !== 'Discord status') p.push(`the button reads "${await btn.innerText()}"`);
  await btn.click();
  const panel = page.locator('dialog.menu.panel[open]');
  if (!(await panel.locator('#mm-presence').count())) p.push('the panel has no presence switch');
  const t = await panel.innerText();
  for (const w of ['0 online', 'null', 'undefined']) if (t.includes(w)) p.push(`the panel shows "${w}"`);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(200);
  return p;
}
// Your room's code (Adam, 2026-10-02): plain letters, normal spacing, with Copy code and Copy invite link beside it.
async function codeLine(page) {
  const p = [];
  const code = page.locator('.on-codebig');
  const look = await code.evaluate((n) => { const c = getComputedStyle(n); return { ls: c.letterSpacing, tt: c.textTransform, size: parseFloat(c.fontSize) }; });
  if (look.ls !== 'normal') p.push(`the room code is letter-spaced (${look.ls})`);
  if (look.tt !== 'none') p.push(`the room code is transformed (${look.tt})`);
  if (look.size < 20 || look.size > 32) p.push(`the room code is ${look.size}px`);
  if (await page.locator('section[aria-label="Your room"] .k-label', { hasText: 'Room code' }).count()) p.push('the Room code label is still there');
  const c = await code.boundingBox(), b = await page.locator('[data-key="copy-code"]').boundingBox();
  if (!c || !b || b.x < c.x + c.width || Math.abs((b.y + b.height / 2) - (c.y + c.height / 2)) > 8) p.push('Copy code is not beside the code');
  return p;
}
// Signed out: the leaderboard is the anonymous answer, so no row is "you" (spec §4).
const noYouRow = async (page) => ((await page.locator('.on-lb-row.you').count()) ? ['a signed-out page highlights a "you" row'] : []);
// Hiding and showing the page re-polls the 60 s leaderboard beat at once (live-common.js makePoller).
const repoll = (page) => page.evaluate(() => {
  let state = 'hidden';
  Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => state });
  document.dispatchEvent(new Event('visibilitychange'));
  state = 'visible';
  document.dispatchEvent(new Event('visibilitychange'));
});
const calls = (page, cmd) => page.evaluate((c) => window.__calls.filter((x) => x.cmd === c).map((x) => x.args), cmd);
// Review Focus 1 (ruling P16): a text field keeps its focus empty and its text and focus typed in, across at least two
// redraws each (two forced by `online-rank`, then two of the page's own 1 s ticks).
async function focusKeep(page, label, key, what) {
  const p = [];
  const redraws = async () => {
    for (let i = 0; i < 2; i++) {
      await page.evaluate(() => document.dispatchEvent(new CustomEvent('online-rank', { detail: null }))); // forces a redraw
      await page.waitForTimeout(300);
    }
    await page.waitForTimeout(2100);
  };
  await page.getByLabel(label, { exact: true }).focus();
  await redraws();
  if (await page.evaluate(() => document.activeElement?.dataset.key) !== key) p.push(`an empty ${what} lost focus on a redraw`);
  await page.keyboard.type('AB');
  await redraws();
  if (await page.getByLabel(label, { exact: true }).inputValue() !== 'AB') p.push(`typed text in the ${what} was lost on a redraw`);
  if (await page.evaluate(() => document.activeElement?.dataset.key) !== key) p.push(`a typed ${what} lost focus`);
  return p;
}

// ---- static: tokens, the sweep, the cards' colours ----
{
  const theme = readFileSync(path.join(ui, 'theme.css'), 'utf8');
  const design = readFileSync(path.join(ui, 'DESIGN.md'), 'utf8');
  const problems = [];
  for (const t of ['--tile-scrim', '--vs-scrim', '--pill-bg', '--lock-bg', '--red-wash']) {
    if (!new RegExp(`${t}\\s*:`).test(theme)) problems.push(`theme.css does not define ${t}`);
    if (!design.includes(`\`${t}\``)) problems.push(`DESIGN.md does not name ${t}`);
  }
  if (!/\.sweep::after\{[^}]*animation:sweep /.test(theme)) problems.push('theme.css has no .sweep loop');
  if (!design.includes('`.sweep`')) problems.push('DESIGN.md does not name .sweep');
  // No pictures yet (Adam, 2026-10-02): each card is a plain colour from a token, and the page loads no image for them.
  for (const t of ['--tile-ranked', '--tile-casual', '--tile-private', '--vs-bg']) {
    if (!new RegExp(`${t}\\s*:\\s*linear-gradient`).test(theme)) problems.push(`theme.css does not define ${t} as a gradient`);
    if (!design.includes(`\`${t}\``)) problems.push(`DESIGN.md does not name ${t}`);
  }
  const mmJs = readFileSync(path.join(ui, 'online-mm.js'), 'utf8');
  if (/art\/online\//.test(mmJs) || /el\('img', \{ class: 'on-art'/.test(mmJs)) problems.push('online-mm.js still loads a card picture');
  if (!/dialog\.menu\.panel\{/.test(theme)) problems.push('theme.css has no dialog.menu.panel');
  if (problems.length) fail('static', problems.join('; ')); else ok('static');
}

// ---- every state ----
const EXPECT = {
  'online-layout': {
    text: ['Online', 'Ranked, casual, or a private game with a friend.'],
    check: async (page) => {
      const p = [];
      const lb = await page.locator('#leaderboard-slot').boundingBox();
      if (!lb || Math.round(lb.width) !== 316) p.push(`the leaderboard column is ${lb?.width}px wide, not 316`);
      const mode = await page.locator('#mode-slot').boundingBox(), live = await page.locator('#live-slot').boundingBox();
      if (!mode || !live || mode.y + mode.height > live.y) p.push('the mode area does not sit above Live now');
      if (await page.locator('#direct-dialog').evaluate((d) => d.open)) p.push('the direct connection dialog is open on load');
      // From Task 4 on, the links line shows under Private room only (ruling 6): pick it when the link is hidden.
      if (!(await page.locator('#direct-open').isVisible())) await page.locator('[data-key="mode-private"]').click();
      await page.locator('#direct-open').click();
      if (!(await page.locator('#direct-dialog').evaluate((d) => d.open))) p.push('Direct connection did not open its dialog');
      if (!(await page.getByRole('button', { name: 'Join game', exact: true }).isVisible())) p.push("the dialog does not hold online.js's form");
      await page.locator('#direct-close').click();
      if (await page.locator('#direct-dialog').evaluate((d) => d.open)) p.push('Close did not close the dialog');
      return p;
    },
  },
  'online-small': {
    check: async (page) => {
      const p = [];
      const mode = await page.locator('#mode-slot').boundingBox(), lb = await page.locator('#leaderboard-slot').boundingBox();
      if (!mode || mode.width < 300) p.push(`the mode area is ${mode?.width}px wide at 800x560`);
      if (!lb || !mode || lb.y < mode.y + mode.height) p.push('the leaderboard does not drop below the mode area at 800x560');
      return p;
    },
  },
  'online-netplay-invite': {
    text: ['Invite received: room K7QP2M9X.'],
    check: async (page) => {
      const p = [];
      if (!(await page.locator('#direct-dialog').evaluate((d) => d.open))) p.push('the invite did not open the Direct connection dialog');
      const code = await page.getByLabel('Host code', { exact: true }).inputValue().catch(() => null);
      if (code !== 'K7QP2M9X') p.push(`the dialog's host code is ${JSON.stringify(code)}, not the invite's K7QP2M9X`);
      if (!(await page.getByRole('button', { name: 'Join game', exact: true }).isVisible())) p.push('Join game is not on screen');
      return p;
    },
  },
  // A direct room open on load opens its dialog once; Close keeps it closed across online.js's 750 ms polls (it
  // re-sets `hidden` each time) and the page behind it, the nav included, works again (fix round 1, Task 2).
  'online-room-host': {
    check: async (page) => {
      const p = [];
      const isOpen = () => page.locator('#direct-dialog').evaluate((d) => d.open);
      if (!(await isOpen())) p.push('an open direct room did not open its dialog');
      await page.locator('#direct-close').click();
      await page.waitForTimeout(1600);
      if (await isOpen()) p.push('the dialog reopened itself after Close while the room stayed open');
      const stats = page.locator('.shell-nav a[href="stats.html"]');
      if (await stats.evaluate((a) => !!a.closest('[inert]') || document.querySelector('dialog:modal') !== null)) p.push('the page is still inert behind a modal');
      else await stats.click({ trial: true, timeout: 2000 }).catch(() => p.push('the Stats link in the nav is not clickable'));
      await page.locator('#direct-open').click();
      if (!(await isOpen())) p.push('Direct connection did not reopen the room');
      return p;
    },
  },
  'online-ranked': {
    text: ['Ranked', 'First to two wins. Moves your Elo.', 'Casual', 'One game. Elo stays put.', 'Private room', 'Host a room or join with a code.',
      'Find a ranked game', 'Matched by Elo', '0 searching', '1 searching', '#7', '1,184 Elo', '▲ 16 this week'],
    absent: ['Quick Match', 'Direct connection', 'best of'],
    check: async (page) => {
      const p = [];
      const pressed = await page.locator('.on-tile[aria-pressed="true"]').evaluateAll((n) => n.map((x) => x.dataset.key));
      if (pressed.join() !== 'mode-ranked') p.push(`picked tiles: ${pressed}`);
      // Picking changes only the edge (Adam, 2026-10-02): every card's colour layer looks the same picked or not.
      const looks = () => page.locator('.on-tile .on-art').evaluateAll((n) => n.map((x) => { const c = getComputedStyle(x); return `${c.filter}|${c.opacity}|${c.backgroundImage}`; }));
      const before = await looks();
      if (before.length !== 3) p.push(`${before.length} card colour layers, not 3`);
      if (before.some((x) => !x.startsWith('none|1|') || !x.includes('linear-gradient'))) p.push(`a card is filtered or has no colour: ${before.join(' / ')}`);
      if (new Set(before.map((x) => x.split('|')[2])).size !== 3) p.push('the three cards do not each have their own colour');
      if (await page.locator('img.on-art').count()) p.push('a card still loads a picture');
      const on = await page.locator('[data-key="mode-ranked"]').evaluate((n) => getComputedStyle(n, '::before').borderTopWidth);
      if (on !== '2px') p.push(`the picked tile's border is ${on}, not 2px`);
      const r = await page.locator('[data-key="mode-ranked"]').boundingBox(), c = await page.locator('[data-key="mode-casual"]').boundingBox();
      if (!r || !c || r.height < c.height * 1.8) p.push('Ranked does not span two rows');
      // Your rank comes from the leaderboard's row (it carries your team too): the expensive dashboard is never read.
      if ((await calls(page, 'stats_dashboard')).length) p.push('the page read stats_dashboard');
      if ((await calls(page, 'player_profile')).length) p.push('the page read player_profile although the board carries your team');
      await page.locator('[data-key="mode-casual"]').click();
      if (await page.evaluate(() => localStorage.getItem('mm:mode')) !== 'casual') p.push('picking Casual did not store mm:mode=casual');
      if (!(await page.getByRole('button', { name: 'Find a casual game', exact: true }).isVisible())) p.push('the row did not follow the pick');
      if ((await looks()).join() !== before.join()) p.push('picking Casual changed how the cards look');
      if (await page.locator('[data-key="mode-casual"]').evaluate((n) => getComputedStyle(n, '::before').borderTopWidth) !== '2px') p.push('the picked Casual card has no outline');
      if (await page.locator('[data-key="mode-ranked"]').evaluate((n) => getComputedStyle(n, '::before').borderTopWidth) !== '1px') p.push('the unpicked Ranked card kept the outline');
      await page.locator('[data-key="mode-private"]').click();
      if ((await looks()).join() !== before.join()) p.push('picking Private room changed how the cards look');
      // Back to Ranked, so the screenshot (taken after this check) shows the state's name.
      await page.locator('[data-key="mode-ranked"]').click();
      if (await page.evaluate(() => localStorage.getItem('mm:mode')) !== 'ranked') p.push('picking Ranked again did not store mm:mode=ranked');
      return p;
    },
  },
  'online-casual': {
    text: ['Find a casual game', 'Matched with anyone'],
    check: async (page) => {
      const p = [];
      const pressed = await page.locator('.on-tile[aria-pressed="true"]').evaluateAll((n) => n.map((x) => x.dataset.key));
      if (pressed.join() !== 'mode-casual') p.push(`picked tiles: ${pressed}`);
      // Only the calls from the click on: update-notice.js reads content_status/content_check by itself on load.
      const from = await page.evaluate(() => window.__calls.length);
      await page.getByRole('button', { name: 'Find a casual game', exact: true }).click();
      await page.waitForFunction(() => window.__calls.some((c) => c.cmd === 'mm_queue'), null, { timeout: 4000 }).catch(() => {});
      const order = await page.evaluate((n) => window.__calls.slice(n).map((c) => c.cmd).filter((c) => /^content_|^mm_queue$/.test(c)), from);
      if (order.join() !== 'content_check,content_repair_image,mm_queue') p.push(`Find ran ${order.join(' → ')}`);
      const q = await calls(page, 'mm_queue');
      if (JSON.stringify(q[0]) !== '{"mode":"casual"}') p.push(`mm_queue got ${JSON.stringify(q[0])}`);
      return p;
    },
  },
  'online-private': {
    text: ['Quick game', 'Pickup', 'Host a room', 'Join', 'Send the code to a friend', 'Not on our servers?', 'Direct connection'],
    absent: ['Find a'],
    check: async (page) => {
      const p = [];
      if (await page.evaluate(() => { const m = document.getElementById('main'); return m.scrollHeight > m.clientHeight; })) p.push('#main scrolls vertically at 1280x800');
      if (await page.evaluate(() => localStorage.getItem('mm:mode')) === 'private') p.push('Private room was stored in mm:mode');
      const join = page.getByRole('button', { name: 'Join by code', exact: true });
      if (!(await join.evaluate((n) => n.classList.contains('quiet')))) p.push('Join is not quiet while the code field is empty');
      if ((await join.innerText()).trim() !== 'Join') p.push('the Join button does not read "Join"');
      // A plain field (Adam, 2026-10-02): its placeholder says what it is, and what is typed shows as typed.
      const field = page.getByLabel('Join with a code', { exact: true });
      if (await field.getAttribute('placeholder') !== 'Join with a code') p.push(`the code field's placeholder is ${JSON.stringify(await field.getAttribute('placeholder'))}`);
      const look = await field.evaluate((n) => { const c = getComputedStyle(n); return `${c.textTransform}|${c.letterSpacing}`; });
      if (look !== 'none|normal') p.push(`the code field is styled ${look} (want no transform, normal spacing)`);
      if (await page.getByRole('group', { name: 'Casual or ranked', exact: true }).count()) p.push('the Casual/Ranked room tabs are still there');
      await field.fill('k7qp2m');
      if (await join.evaluate((n) => n.classList.contains('quiet'))) p.push('Join stays quiet with a code typed');
      await join.click();
      await page.waitForTimeout(300);
      const j = await calls(page, 'mm_room_join');
      if (JSON.stringify(j[0]) !== '{"code":"k7qp2m"}') p.push(`mm_room_join got ${JSON.stringify(j[0])}`);
      await page.getByRole('button', { name: 'Host a room', exact: true }).click();
      await page.waitForTimeout(300);
      const h = await calls(page, 'mm_room_create');
      if (JSON.stringify(h[0]) !== '{"mode":"casual","public":false,"seedKind":"regular"}') p.push(`Host a room sent ${JSON.stringify(h[0])}`);
      // Quick game | Pickup (Adam, 2026-10-02): Quick game to start with; Host sends the pick as seedKind.
      const kind = page.getByRole('group', { name: 'Quick game or Pickup', exact: true });
      const was = await kind.locator('.tab[aria-pressed="true"]').innerText().catch(() => null);
      if (was?.trim() !== 'Quick game') p.push(`the game choice starts on ${JSON.stringify(was)}, not Quick game`);
      await kind.getByRole('button', { name: 'Pickup', exact: true }).click();
      const pressed = await page.locator('.on-tile[aria-pressed="true"]').evaluateAll((n) => n.map((x) => x.dataset.key));
      if (pressed.join() !== 'mode-private') p.push(`picking Pickup moved the tile pick to ${pressed}`);
      await page.getByRole('button', { name: 'Host a room', exact: true }).click();
      await page.waitForTimeout(300);
      const h2 = await calls(page, 'mm_room_create');
      if (JSON.stringify(h2[1]) !== '{"mode":"casual","public":false,"seedKind":"pickup"}') p.push(`Host a room after picking Pickup sent ${JSON.stringify(h2[1])}`);
      // Back to Quick game, so the screenshot shows the default.
      await kind.getByRole('button', { name: 'Quick game', exact: true }).click();
      return p;
    },
  },
  'online-searching': {
    text: ['Looking for players near 1,184 Elo', 'Searching… 0:42', 'Cancel'],
    absent: ['Find a ranked game', 'Not on our servers?'],
    check: async (page) => {
      const p = [];
      if ((await page.locator('.on-tile.dim').count()) !== 2) p.push('the other two tiles are not dark');
      if ((await page.locator('.on-search .sweep').count()) !== 1) p.push('no sweeping line on the search bar');
      if ((await page.locator('.on-search').innerText()).toLowerCase().indexOf('ranked') < 0) p.push('the bar does not say Ranked');
      await page.getByRole('button', { name: 'Cancel', exact: true }).click();
      await page.waitForTimeout(300);
      if (!(await calls(page, 'mm_leave_queue')).length) p.push('Cancel did not send mm_leave_queue');
      return p;
    },
  },
  'online-match-found': {
    text: ['Match found', 'VS', 'Ranked · first to two wins', 'Accept within 0:14', 'Jock', '#1 · 1,320 Elo', 'Adam', '#7 · 1,184 Elo'],
    absent: ['best of', 'Not on our servers?'],
    check: async (page) => {
      const p = [];
      for (const b of ['Accept', 'Decline', 'Leave match']) if (!(await page.getByRole('button', { name: b, exact: true }).count())) p.push(`no ${b} button`);
      // Ruling P8: Decline is the quiet button (spec §3.3).
      if (!(await page.locator('[data-key="decline"].btn.quiet').count())) p.push('Decline is not a quiet button');
      if (await page.locator('.on-vs .sweep').count()) p.push('the line sweeps before anyone accepted');
      if ((await page.locator('.on-vs .team-logo').count()) !== 2) p.push('both plays-as team logos should show');
      if (!(await page.locator('.on-tm[data-deadline]').count())) p.push('Accept within carries no deadline for the ticker');
      await page.getByRole('button', { name: 'Accept', exact: true }).click();
      await page.waitForTimeout(300);
      const a = await calls(page, 'mm_match_action');
      if (JSON.stringify(a[0]) !== '{"matchId":"m5","action":"accept","value":true}') p.push(`Accept sent ${JSON.stringify(a[0])}`);
      return p;
    },
  },
  'online-match-found-tick': {
    text: ['Accept within 0:14'],
    check: async (page) => {
      const p = [];
      const tm = page.locator('.on-tm[data-deadline]');
      // The page's clock is frozen at 20:00:00; from here it runs, so the 1 s ticker has something to count.
      await page.evaluate(() => { const base = Date.now(), t0 = performance.now(); Date.now = () => base + (performance.now() - t0); });
      const before = await tm.innerText();
      await page.waitForTimeout(2200);
      const after = await tm.innerText();
      if (before === after) p.push(`Accept within did not count down: "${before}" stayed "${after}"`);
      if (!/^Accept within 0:1[12]$/.test(after.trim())) p.push(`after 2.2 s it reads "${after}"`);
      // Past the deadline (20:00:14): the line hides.
      await page.evaluate(() => { Date.now = () => Date.UTC(2026, 8, 30, 20, 0, 15); });
      await page.waitForTimeout(1200);
      if (await tm.isVisible()) p.push(`Accept within still shows past the deadline ("${await tm.innerText()}")`);
      if (!(await page.getByRole('button', { name: 'Accept', exact: true }).isVisible())) p.push('Accept went away with the timer');
      return p;
    },
  },
  'online-accepted': {
    text: ['Accepted. Waiting for Jock…'],
    check: async (page) => {
      const p = [];
      if (!(await page.locator('.on-ok').isVisible())) p.push('no green tick on your side');
      if (await page.getByRole('button', { name: 'Accept', exact: true }).count()) p.push('Accept is still offered');
      return p;
    },
  },
  'online-starting': {
    text: ['Connecting you both', 'Your game opens on its own'],
    check: async (page) => {
      const p = [];
      if ((await page.locator('.on-vs .sweep').count()) !== 1) p.push('no sweeping line while the relay moves');
      await page.getByRole('button', { name: 'Leave match', exact: true }).click();
      await page.waitForTimeout(300);
      const a = await calls(page, 'mm_match_action');
      if (JSON.stringify(a[0]) !== '{"matchId":"m5","action":"leave","value":null}') p.push(`Leave match sent ${JSON.stringify(a[0])}`);
      return p;
    },
  },
  'online-playing': { text: ['Playing', 'Game 2 · Series 1–0', 'Ranked · first to two wins', 'gg'], absent: ['Accept within', 'Your game opens', 'Match found'] },
  // Ruling P9: intermission is still "Playing", with the series line.
  'online-intermission': { text: ['Playing', 'Between games', 'Game 2 · Series 1–0', 'Ranked · first to two wins'], absent: ['Match found', 'Accept within'] },
  'online-playing-casual': { text: ['Playing', 'Casual · one game'], absent: ['Series', 'Match found'] },
  'online-match-bare': {
    text: ['Opponent'], absent: ['undefined', 'NaN', 'null', 'Accept within', ' · Elo'],
    check: async (page) => {
      const p = [];
      const names = await page.locator('.on-vs .on-pname').allInnerTexts();
      if (names[1] !== 'Opponent') p.push(`the nameless opponent is drawn as ${JSON.stringify(names[1])}, not "Opponent"`);
      const letter = (await page.locator('.on-vs .on-av').nth(1).innerText()).trim();
      if (letter !== 'O') p.push(`the nameless opponent's letter is ${JSON.stringify(letter)}, not "O"`);
      return p;
    },
  },
  // The matchup card at the minimum window: one column, the faces clear of Leave match (generic check below).
  'online-match-small': { text: ['Match found', 'Jock', 'Accept within 0:14'] },
  'online-room': {
    text: ['Your room · Quick game', 'K7QP2M', 'Copy code', 'Copy invite link', 'Players', 'Adam', 'you', 'Ready', 'Not ready', 'Chat', 'ready when you are',
      "Start works once you're both ready. The host starts the game."],
    absent: ['Lobby code', 'Leave lobby', 'Room code', 'Pickup'],
    check: async (page) => {
      const p = await codeLine(page);
      // Ruling P14: count the markers, not loose text ("you"/"Ready" appear in unrelated words).
      const rd = await page.locator('.on-rd').count(), nr = await page.locator('.on-nr').count();
      if (rd !== 1 || nr !== 1) p.push(`ready markers: ${rd} ready, ${nr} not ready (want 1 and 1)`);
      // Layout (1280x800): Start, the ready toggle and Leave room share one row; the chat field sits at the bottom of its card.
      const boxes = await page.evaluate(() => ['room-start', 'room-ready', 'room-leave'].map((k) => { const r = document.querySelector(`[data-key="${k}"]`)?.getBoundingClientRect(); return r ? [r.top, r.height] : null; }));
      if (boxes.some((x) => !x)) p.push('a room button is missing');
      else {
        const tops = boxes.map((x) => x[0]), hs = boxes.map((x) => x[1]);
        if (Math.max(...tops) - Math.min(...tops) > 2) p.push(`room buttons are not on one row: tops ${tops}`);
        if (Math.max(...hs) - Math.min(...hs) > 2) p.push(`room buttons differ in height: ${hs}`);
      }
      const gap = await page.evaluate(() => { const i = document.querySelector('[data-key="chat"]').getBoundingClientRect().bottom, c = document.querySelector('section[aria-label="Chat"]').getBoundingClientRect().bottom; return c - i; });
      if (gap > 24) p.push(`chat input is ${gap}px above the bottom of the Chat card`);
      if (!(await page.getByRole('button', { name: 'Start', exact: true }).isDisabled())) p.push('Start is enabled while Jock is not ready');
      await page.getByRole('button', { name: 'Copy invite link', exact: true }).click();
      await page.waitForTimeout(200);
      if (await page.evaluate(() => window.__copied) !== 'https://nflstreetreborn.com/join.php?r=K7QP2M') p.push(`copied ${await page.evaluate(() => window.__copied)}`);
      await page.getByRole('button', { name: 'Copy code', exact: true }).click();
      await page.waitForTimeout(200);
      if (await page.evaluate(() => window.__copied) !== 'K7QP2M') p.push('Copy code did not copy the code');
      await page.getByRole('button', { name: "I'm not ready", exact: true }).click();
      await page.waitForTimeout(300);
      const r = await calls(page, 'mm_room_action');
      if (JSON.stringify(r[0]) !== '{"roomId":"r1","action":"ready","value":false}') p.push(`ready sent ${JSON.stringify(r[0])}`);
      await page.getByRole('button', { name: 'Leave room', exact: true }).click();
      await page.waitForTimeout(300);
      const l = (await calls(page, 'mm_room_action')).at(-1);
      if (JSON.stringify(l) !== '{"roomId":"r1","action":"leave","value":null}') p.push(`Leave room sent ${JSON.stringify(l)}`);
      return p;
    },
  },
  'online-room-pickup': { text: ['Your room · Pickup', 'K7QP2M'], absent: ['Quick game', 'Room code'], check: codeLine },
  'online-held': {
    text: ['Your last match is held for review', "Your game files differ from Jock's, so that game was not started.", 'Find a casual game'],
    absent: ['Leave match', 'Accept'],
    check: async (page) => {
      const p = [];
      const notice = page.locator('.on-notice');
      if (!(await notice.isVisible())) return ['no held notice'];
      const n = await notice.boundingBox(), t = await page.locator('.on-modes').boundingBox();
      if (!t || n.y > t.y) p.push('the notice is not above the tiles');
      await page.getByRole('button', { name: 'Dismiss', exact: true }).click();
      await page.waitForTimeout(300);
      if (await page.locator('.on-notice').count()) p.push('Dismiss did not hide the notice');
      return p;
    },
  },
  'online-held-other': { text: ['Nothing changed on the leaderboard. Someone will look at it. You can play again now.'], absent: ['ladder'] },
  'online-signed-out': {
    text: ['Sign in to play online', 'Connect your Discord account to play online and track your stats!', 'Sign in with Discord', 'Live now', 'Direct connection'],
    absent: ['Find a', 'searching', '1,184 Elo', '#7', '0 online'],
    check: async (page) => {
      const p = [...(await noYouRow(page)), ...(await discordStatus(page))];
      if (!(await page.locator('.on-modes[inert]').count())) p.push('the tiles behind the panel can still be reached');
      await page.getByRole('button', { name: 'Sign in with Discord', exact: true }).click();
      await page.getByText('Your code is ABCD-1234', { exact: true }).waitFor({ timeout: 3000 }).catch(() => p.push('the device code is not shown'));
      if (!(await calls(page, 'identity_sign_in_start')).length) p.push('Sign in did not start the device-code flow');
      await page.locator('[data-key="signin-cancel"]').click();
      await page.getByRole('button', { name: 'Sign in with Discord', exact: true }).waitFor({ timeout: 3000 }).catch(() => p.push('Cancel did not return to the Sign in button'));
      return p;
    },
  },
  'online-connecting': { text: ['Connecting…', 'Discord status'], absent: ['null', 'undefined', '0 online'], check: discordStatus },
  'online-no-online-list': { text: ['Find a ranked game', 'Discord status'], absent: ['null', 'undefined', ' online'], check: discordStatus },
  'online-offline': { text: ["Can't reach the Reborn servers right now.", 'Trying again.', 'Discord status'], absent: ['Sign in with Discord', 'Find a', '0 online'], check: discordStatus },
  'online-disabled': {
    text: ['Private room', 'Direct connection'],
    absent: ['Find a', "Can't reach", 'Sign in', 'Not on our servers', '0 online'],
    check: async (page) => {
      const p = [];
      if (await page.locator('#on-players').isVisible()) p.push('the Discord status button shows with matchmaking off');
      if ((await page.locator('.on-tile').count()) !== 1) p.push(`${await page.locator('.on-tile').count()} tiles with matchmaking off`);
      await page.locator('.on-row [data-key="direct"]').click();
      if (!(await page.locator('#direct-dialog').evaluate((d) => d.open))) p.push('the Direct connection button did not open the dialog');
      return p;
    },
  },
  'online-presence': {
    text: ['3 players online'],
    check: async (page) => {
      const p = [];
      await page.locator('#on-players').click();
      const panel = page.locator('dialog.menu.panel[open]');
      if (!(await panel.isVisible())) return ['the players button opened nothing'];
      await page.screenshot({ path: path.join(out, 'online-presence-open.png') });
      const t = await panel.innerText();
      for (const w of ['Adam', 'Jock', 'Tuna', 'Looking for a game', "Show what I'm doing on Discord"]) if (!t.includes(w)) p.push(`the panel lacks "${w}"`);
      const sw = panel.locator('#mm-presence');
      if (await sw.getAttribute('aria-checked') !== 'true') p.push('the switch does not show presence on');
      await sw.click();
      await page.waitForTimeout(300);
      const s = await calls(page, 'presence_set_enabled');
      if (JSON.stringify(s[0]) !== '{"enabled":false}') p.push(`the switch sent ${JSON.stringify(s[0])}`);
      await page.keyboard.press('Escape');
      await page.waitForTimeout(200);
      if (await page.locator('dialog.menu.panel[open]').count()) p.push('Esc did not close the panel');
      if (await page.evaluate(() => document.activeElement?.id) !== 'on-players') p.push('focus did not return to the players button');
      return p;
    },
  },
  'online-lobbies': {
    text: ['Open lobbies (2) ›'],
    check: async (page) => {
      const p = [];
      await page.locator('#lobbies-open').click();
      const panel = page.locator('dialog.menu.panel[open]');
      const t = await panel.innerText().catch(() => '');
      for (const w of ['Jock · Casual', 'Tuna · Ranked', 'Open a public lobby']) if (!t.includes(w)) p.push(`the lobby panel lacks "${w}"`);
      if (/best of/i.test(t)) p.push('a lobby is labelled best of');
      await page.screenshot({ path: path.join(out, 'online-lobbies-panel.png') });
      await panel.getByRole('button', { name: 'Join', exact: true }).first().click();
      await page.waitForTimeout(300);
      const j = await calls(page, 'mm_room_join');
      if (JSON.stringify(j[0]) !== '{"code":"K7QP2M"}') p.push(`Join sent ${JSON.stringify(j[0])}`);
      return p;
    },
  },
  'online-lobbies-none': {
    text: ['Open a public lobby ›'],
    check: async (page) => {
      const p = [];
      await page.locator('#lobbies-open').click();
      const panel = page.locator('dialog.menu.panel[open]');
      if (!(await panel.innerText().catch(() => '')).includes('No open lobbies. Open one and someone can join you.')) p.push('no "No open lobbies" line');
      await page.screenshot({ path: path.join(out, 'online-lobbies-none-panel.png') });
      await panel.getByRole('button', { name: 'Open a public lobby', exact: true }).click();
      await page.waitForTimeout(300);
      const c = await calls(page, 'mm_room_create');
      if (JSON.stringify(c[0]) !== '{"mode":"casual","public":true}') p.push(`Open a public lobby sent ${JSON.stringify(c[0])}`);
      return p;
    },
  },
  'online-live': {
    text: ['Live now', 'See all', 'Ranked · Game 1', 'Casual', 'Watch', 'Giants', 'Colts', 'Adam', 'Jock', '2 live games'],
    absent: ['Game 1 of 3'],
    check: async (page) => {
      const p = [];
      if ((await page.locator('.live-row').count()) !== 2) p.push(`${await page.locator('.live-row').count()} live cards`);
      if ((await page.locator('.live-row').first().locator('.live-side').count()) !== 2) p.push('a card does not have two team rows');
      const href = await page.locator('.live-watch').first().getAttribute('href');
      if (!/^live\.html\?match=/.test(href || '')) p.push(`Watch goes to ${href}`);
      const live = await page.locator('#live-slot').boundingBox(), mode = await page.locator('#mode-slot').boundingBox();
      if (!live || !mode || live.y < mode.y + mode.height) p.push('Live now is not under the mode area');
      await page.getByRole('button', { name: 'See all ›', exact: true }).click();
      await page.waitForTimeout(200);
      const o = await calls(page, 'open_website');
      if (JSON.stringify(o[0]) !== '{"url":"https://online.nflstreetreborn.com/live"}') p.push(`See all opened ${JSON.stringify(o[0])}`);
      return p;
    },
  },
  'online-live-many': {
    text: ['5 live games'],
    check: async (page) => ((await page.locator('.live-row').count()) === 3 ? [] : [`${await page.locator('.live-row').count()} cards, not 3`]),
  },
  'online-live-none': {
    text: ['Live now', 'No live games right now.'],
    absent: ['0 live game'],
    check: async (page) => {
      const p = [];
      if (await page.locator('#on-live-count').isVisible()) p.push('the heading shows a live-games count with no live games');
      if (await page.locator('.live-row').count()) p.push('live cards are drawn');
      return p;
    },
  },
  'online-leaderboard': {
    text: ['Leaderboard', '38 ranked', 'Player', 'Elo', 'Jock', '20–3', '1,320', '▲ 16', '▼ 9', 'Full leaderboard ›'],
    check: async (page) => {
      const p = [];
      if ((await page.locator('.on-lb-row').count()) !== 10) p.push(`${await page.locator('.on-lb-row').count()} rows, not 10`);
      if ((await page.locator('.on-lb-row.top').count()) !== 3) p.push('the top three are not marked');
      const you = page.locator('.on-lb-row.you');
      if ((await you.count()) !== 1 || (await you.locator('.on-lb-rank').innerText()).trim() !== '7') p.push('your row (#7) is not marked');
      if (await page.locator('.on-lb-gap').count()) p.push('a gap row while you are in the top 10');
      const srcs = await page.locator('#leaderboard-slot img').evaluateAll((n) => n.map((x) => x.getAttribute('src')));
      for (const s of srcs) if (!/^https:\/\/(api\.nflstreetreborn\.com\/live-assets\/|cdn\.discordapp\.com\/avatars\/)/.test(s)) p.push(`image from ${s}`);
      const card = await page.locator('#leaderboard-slot').boundingBox(), main = await page.locator('#main').boundingBox();
      if (!card || card.y + card.height > main.y + main.height + 1) p.push('the leaderboard card runs off the window');
      await page.getByRole('button', { name: 'Full leaderboard ›', exact: true }).click();
      await page.waitForTimeout(200);
      const o = await calls(page, 'open_website');
      if (JSON.stringify(o.at(-1)) !== '{"url":"https://online.nflstreetreborn.com/leaderboard"}') p.push(`Full leaderboard opened ${JSON.stringify(o.at(-1))}`);
      return p;
    },
  },
  'online-leaderboard-far': {
    text: ['1,104', '#14', 'Dime'],
    check: async (page) => {
      const p = [];
      if ((await page.locator('.on-lb-gap').count()) !== 1) p.push('no gap before your row');
      const last = page.locator('.on-lb-row').last();
      if (!(await last.evaluate((n) => n.classList.contains('you')))) p.push('the last row is not yours');
      if ((await last.locator('.on-lb-rank').innerText()).trim() !== '14') p.push('your row does not say 14');
      return p;
    },
  },
  'online-rank-blip': {
    text: ['#7', '1,184 Elo'],
    check: async (page) => {
      const p = [];
      const before = (await calls(page, 'leaderboard')).length;
      // The next leaderboard read fails; hiding and showing the page re-polls at once (live-common.js makePoller).
      await page.evaluate(() => {
        const core = window.__TAURI__.core, real = core.invoke;
        core.invoke = (cmd, args) => (cmd === 'leaderboard' ? (window.__calls.push({ cmd, args }), Promise.reject({ kind: 'unavailable', message: 'Live games couldn\'t be loaded right now.' })) : real(cmd, args));
      });
      await repoll(page);
      await page.waitForTimeout(400);
      if ((await calls(page, 'leaderboard')).length <= before) p.push('the page did not read leaderboard again');
      const tile = await page.locator('[data-key="mode-ranked"]').innerText();
      if (!tile.includes('#7') || !tile.includes('1,184 Elo')) p.push(`a failed leaderboard read blanked the Ranked tile's rank (${tile.replace(/\s+/g, ' ')})`);
      if (!(await page.locator('.on-lb-row.you').count())) p.push('a failed leaderboard read blanked the card');
      return p;
    },
  },
  'online-elo-off': {
    text: ['Leaderboard', 'Rankings start soon.', 'Find a ranked game'],
    absent: ['Full leaderboard', '38 ranked', '1,184 Elo', '#7', 'this week', 'Looking for players near'],
    check: async (page) => {
      const p = [];
      // No row of yours: the plays-as team comes from ONE player_profile read per page load, even across beats.
      await repoll(page);
      await page.waitForTimeout(400);
      const prof = await calls(page, 'player_profile');
      if (prof.length !== 1) p.push(`player_profile was read ${prof.length} times, not once`);
      else if (typeof prof[0].playerId !== 'string' || !prof[0].playerId) p.push(`player_profile got ${JSON.stringify(prof[0])}`);
      if ((await calls(page, 'stats_dashboard')).length) p.push('the page read stats_dashboard');
      if (await page.locator('[data-key="mode-ranked"] .on-rank').count()) p.push('the Ranked tile shows a rank with Elo off');
      return p;
    },
  },
  'online-lb-missing': { text: ['Rankings start soon.'], absent: ['Full leaderboard'] },
  'online-lb-hostile': {
    text: ['<img src=x onerror=alert(1)>', 'Averyveryvery'],
    absent: ['undefined', 'NaN', 'null'],
    check: async (page) => {
      const p = [];
      if (await page.locator('#leaderboard-slot img[src="x"]').count()) p.push('a hostile name became an element');
      if (await page.locator('#leaderboard-slot img[src^="https://evil"]').count()) p.push('an image from outside the two allowed prefixes');
      // Ruling P5: the long name is cut with an ellipsis; the no-sideways-scroll check below runs for every state.
      const long = page.locator('.on-lb-name').nth(1);
      if (!(await long.evaluate((n) => n.scrollWidth > n.clientWidth))) p.push('the long name is not cut with an ellipsis');
      return p;
    },
  },
  'online-invite': {
    text: ['Jock invited you to a room · K7QP2M', 'Host a room'],
    check: async (page) => {
      const p = [];
      if ((await calls(page, 'mm_room_join')).length) p.push('the invite joined on its own');
      if (await page.locator('[data-key="mode-private"]').getAttribute('aria-pressed') !== 'true') p.push('Private room is not picked');
      if (await page.getByLabel('Join with a code', { exact: true }).inputValue() !== 'K7QP2M') p.push('the code field is not filled');
      await page.locator('[data-key="invite-join"]').click();
      await page.waitForTimeout(400);
      const j = await calls(page, 'mm_room_join');
      if (JSON.stringify(j[0]) !== '{"code":"K7QP2M"}') p.push(`Join sent ${JSON.stringify(j[0])}`);
      if (!(await calls(page, 'mm_pending_room_clear')).length) p.push('a joined invite was not cleared');
      return p;
    },
  },
  'online-invite-nohost': {
    text: ['You were invited to a room · K7QP2M'],
    check: async (page) => {
      const p = [];
      // No host is an answer: focus does not ask again for the same code.
      await page.evaluate(() => window.dispatchEvent(new Event('focus')));
      await page.waitForTimeout(300);
      if ((await calls(page, 'mm_room_lookup')).length !== 1) p.push(`the lookup was asked ${(await calls(page, 'mm_room_lookup')).length} times`);
      await page.locator('[data-key="invite-dismiss"]').click();
      await page.waitForTimeout(300);
      if (!(await calls(page, 'mm_pending_room_clear')).length) p.push('Dismiss did not clear the invite');
      if (await page.locator('.on-invite').count()) p.push('the invite line is still there');
      return p;
    },
  },
  'online-invite-signed-out': {
    text: ['Sign in to join K7QP2M', 'Sign in with Discord', 'Not now'],
    check: async (page) => [...(await noYouRow(page)), ...((await calls(page, 'mm_pending_room_clear')).length || (await calls(page, 'mm_room_join')).length ? ['a signed-out invite was used up'] : [])],
  },
  'online-invite-not-now': {
    text: ['Sign in to join K7QP2M'],
    check: async (page) => {
      const p = await noYouRow(page);
      await page.locator('[data-key="invite-notnow"]').click();
      await page.waitForTimeout(300);
      if (JSON.stringify(await calls(page, 'mm_pending_room_clear')) !== '[{}]') p.push('Not now did not clear the invite');
      if (await page.getByText('Sign in to join K7QP2M').count()) p.push('the lock still names the invite');
      if (!(await page.getByText('Sign in to play online').count())) p.push('the lock did not go back to its own title');
      if ((await calls(page, 'mm_room_join')).length) p.push('Not now joined');
      // The file is still answered by the stub: a focus re-read must not bring it back.
      await page.evaluate(() => window.dispatchEvent(new Event('focus')));
      await page.waitForTimeout(300);
      if (await page.getByText('Sign in to join K7QP2M').count()) p.push('a focus re-read brought the dismissed invite back');
      return p;
    },
  },
  'online-invite-long': {
    absent: ['invited you', 'You were invited'],
    check: async (page) => ((await calls(page, 'mm_room_lookup')).length ? ['a 13-character code was taken as an invite'] : []),
  },
  'online-invite-busy': {
    absent: ['invited you', 'You were invited'],
    check: async (page) => ((await calls(page, 'mm_pending_room_clear')).length ? ['the invite was dropped while queued'] : []),
  },
  'online-netplay-invite-busy': {
    check: async (page) => ((await page.locator('#direct-dialog[open]').count()) ? ['the Direct connection dialog opened while queued'] : []),
  },
  'online-pad': {
    check: async (page) => {
      const p = [];
      // A controller drives: the shell puts gamepad-input on <body> and focuses with .pad-focus.
      await page.evaluate(() => { document.body.classList.add('gamepad-input'); document.querySelector('[data-key="mode-casual"]').focus(); });
      await page.waitForTimeout(500);
      if (await page.locator('[data-key="mode-casual"]').getAttribute('aria-pressed') !== 'true') p.push('the pick did not follow focus to Casual');
      if (await page.evaluate(() => document.activeElement?.dataset.key) !== 'mode-casual') p.push('focus left the tile after the redraw');
      const offset = await page.evaluate(() => getComputedStyle(document.activeElement).outlineOffset);
      if (offset !== '4px') p.push(`the tile's focus ring sits ${offset} out, not 4px`);
      await page.screenshot({ path: path.join(out, 'online-pad-focus.png') });
      await page.evaluate(() => document.activeElement.click()); // A
      await page.waitForFunction(() => window.__calls.some((c) => c.cmd === 'content_check'), null, { timeout: 3000 }).catch(() => p.push('A on the picked tile did not start Find a game'));
      return p;
    },
  },
  'online-searching-esc': {
    check: async (page) => {
      await page.keyboard.press('Escape');
      await page.waitForTimeout(300);
      return (await calls(page, 'mm_leave_queue')).length ? [] : ['Esc while searching did not Cancel'];
    },
  },
  // The Discord status panel open while Searching: Esc closes the panel only; it is not the Cancel button.
  'online-searching-panel-esc': {
    check: async (page) => {
      const p = [];
      await page.locator('#on-players').click();
      if (!(await page.locator('dialog.menu.panel[open]').count())) p.push('the Discord status panel did not open');
      await page.keyboard.press('Escape');
      await page.waitForTimeout(400);
      if (await page.locator('dialog.menu.panel[open]').count()) p.push('Esc did not close the panel');
      if ((await calls(page, 'mm_leave_queue')).length) p.push('Esc in the panel also cancelled the search');
      return p;
    },
  },
  'online-match-found-esc': {
    check: async (page) => {
      await page.keyboard.press('Escape');
      await page.waitForTimeout(300);
      const a = await calls(page, 'mm_match_action');
      return JSON.stringify(a[0]) === '{"matchId":"m5","action":"accept","value":false}' ? [] : [`Esc on Match found sent ${JSON.stringify(a[0])}`];
    },
  },
  'online-focus-keep': {
    check: async (page) => focusKeep(page, 'Join with a code', 'room-code', 'code field'),
  },
  'online-focus-keep-chat': {
    check: async (page) => focusKeep(page, 'Chat message', 'chat', 'chat field'),
  },
  // (states are added above this line)
};

const byName = Object.fromEntries((STATES.online || []).map((s) => [s.name, s]));
if (Object.keys(EXPECT).length) {
  const server = await serve();
  const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
  try {
    for (const [name, exp] of Object.entries(EXPECT)) {
      const state = byName[name];
      if (!state) { fail(name, 'no such state in Tools/lib/page-states.mjs'); continue; }
      const { page, errors, close } = await openState(browser, server.base, state, { pad: !!state.pad, init: state.init ?? null, viewport: state.viewport });
      const problems = [];
      try {
        const text = (await page.locator('body').innerText()).toLowerCase();
        for (const t of exp.text ?? []) if (!text.includes(t.toLowerCase())) problems.push(`missing text "${t}"`);
        for (const t of exp.absent ?? []) if (text.includes(t.toLowerCase())) problems.push(`should not show "${t}"`);
        if (exp.check) problems.push(...(await exp.check(page)));
        if (await page.locator('.on-vs .on-leave').count()) {
          const leave = await page.locator('.on-vs .on-leave').boundingBox();
          const avs = await page.locator('.on-vs .on-av').evaluateAll((xs) => xs.map((x) => { const r = x.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; }));
          const tops = await page.locator('.on-vs .on-avw').evaluateAll((xs) => xs.map((x) => x.getBoundingClientRect().top));
          if (tops.length === 2 && Math.abs(tops[0] - tops[1]) > 1) problems.push(`the two faces are not level (${tops[0]} vs ${tops[1]})`);
          if (avs.length !== 2) problems.push(`the matchup card has ${avs.length} avatars, not 2`);
          for (const a of avs) {
            if (leave && leave.x < a.x + a.width && a.x < leave.x + leave.width && leave.y < a.y + a.height && a.y < leave.y + leave.height) problems.push('Leave match overlaps a face');
          }
        }
        if (await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)) problems.push('the page scrolls sideways');
        await page.screenshot({ path: path.join(out, `${name}.png`) });
      } catch (e) {
        problems.push(`threw: ${e.message.split('\n')[0]}`);
      } finally {
        await close();
      }
      if (errors.length) problems.push(`page errors: ${errors.join(' | ')}`);
      if (problems.length) fail(name, problems.join('; ')); else ok(name);
    }
  } finally {
    await browser.close();
    await server.close();
  }
}
if (failures) { console.error(`check-online-page: FAIL (${failures})`); process.exit(1); }
console.log(`check-online-page: PASS (${Object.keys(EXPECT).length} state(s))`);
