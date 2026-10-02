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
const calls = (page, cmd) => page.evaluate((c) => window.__calls.filter((x) => x.cmd === c).map((x) => x.args), cmd);

// ---- static: tokens, the sweep, the art ----
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
  // A JPEG's width from its first SOFn marker (no image library in Tools).
  const jpegWidth = (buf) => {
    let i = 2;
    while (i + 9 < buf.length && buf[i] === 0xff) {
      const m = buf[i + 1], len = buf.readUInt16BE(i + 2);
      if (m >= 0xc0 && m <= 0xcf && ![0xc4, 0xc8, 0xcc].includes(m)) return buf.readUInt16BE(i + 7);
      i += 2 + len;
    }
    return null;
  };
  for (const f of ['ranked', 'casual', 'private', 'matchup']) {
    const p = path.join(ui, `art/online/${f}.jpg`);
    if (!existsSync(p)) { problems.push(`art/online/${f}.jpg is missing`); continue; }
    const w = jpegWidth(readFileSync(p));
    if (!w || w > 1100) problems.push(`art/online/${f}.jpg is ${w}px wide (a display copy is at most 1100)`);
  }
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
      'Find a ranked game', 'Matched by Elo', '0 searching', '1 searching'],
    absent: ['Quick Match', 'Direct connection', 'best of'],
    check: async (page) => {
      const p = [];
      const pressed = await page.locator('.on-tile[aria-pressed="true"]').evaluateAll((n) => n.map((x) => x.dataset.key));
      if (pressed.join() !== 'mode-ranked') p.push(`picked tiles: ${pressed}`);
      const art = await page.locator('[data-key="mode-casual"] .on-art').evaluate((n) => getComputedStyle(n).filter);
      if (!/saturate/.test(art)) p.push(`an unpicked tile's picture is in full colour (${art})`);
      const on = await page.locator('[data-key="mode-ranked"]').evaluate((n) => getComputedStyle(n, '::before').borderTopWidth);
      if (on !== '2px') p.push(`the picked tile's border is ${on}, not 2px`);
      const r = await page.locator('[data-key="mode-ranked"]').boundingBox(), c = await page.locator('[data-key="mode-casual"]').boundingBox();
      if (!r || !c || r.height < c.height * 1.8) p.push('Ranked does not span two rows');
      await page.locator('[data-key="mode-casual"]').click();
      if (await page.evaluate(() => localStorage.getItem('mm:mode')) !== 'casual') p.push('picking Casual did not store mm:mode=casual');
      if (!(await page.getByRole('button', { name: 'Find a casual game', exact: true }).isVisible())) p.push('the row did not follow the pick');
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
    text: ['Host a room', 'Send the code to a friend', 'Not on our servers?', 'Direct connection'],
    absent: ['Find a'],
    check: async (page) => {
      const p = [];
      if (await page.evaluate(() => localStorage.getItem('mm:mode')) === 'private') p.push('Private room was stored in mm:mode');
      const join = page.getByRole('button', { name: 'Join by code', exact: true });
      if (!(await join.evaluate((n) => n.classList.contains('quiet')))) p.push('Join is not quiet while the code field is empty');
      if ((await join.innerText()).trim() !== 'Join') p.push('the Join button does not read "Join"');
      await page.getByLabel('Room code', { exact: true }).fill('k7qp2m');
      if (await join.evaluate((n) => n.classList.contains('quiet'))) p.push('Join stays quiet with a code typed');
      await join.click();
      await page.waitForTimeout(300);
      const j = await calls(page, 'mm_room_join');
      if (JSON.stringify(j[0]) !== '{"code":"k7qp2m"}') p.push(`mm_room_join got ${JSON.stringify(j[0])}`);
      await page.getByRole('button', { name: 'Host a room', exact: true }).click();
      await page.waitForTimeout(300);
      const h = await calls(page, 'mm_room_create');
      if (JSON.stringify(h[0]) !== '{"mode":"casual","public":false}') p.push(`Host a room sent ${JSON.stringify(h[0])}`);
      // Ruling P20: the room's mode is picked here (the old Quick Match tabs' job), stored in mm:mode, and Host sends it.
      const hostMode = page.getByRole('group', { name: 'Casual or ranked', exact: true });
      const was = await hostMode.locator('.tab[aria-pressed="true"]').innerText().catch(() => null);
      if (was?.trim() !== 'Casual') p.push(`the room mode tabs start on ${JSON.stringify(was)}, not Casual`);
      await hostMode.getByRole('button', { name: 'Ranked', exact: true }).click();
      if (await page.evaluate(() => localStorage.getItem('mm:mode')) !== 'ranked') p.push('picking Ranked for the room did not store mm:mode=ranked');
      const pressed = await page.locator('.on-tile[aria-pressed="true"]').evaluateAll((n) => n.map((x) => x.dataset.key));
      if (pressed.join() !== 'mode-private') p.push(`picking the room mode moved the tile pick to ${pressed}`);
      await page.getByRole('button', { name: 'Host a room', exact: true }).click();
      await page.waitForTimeout(300);
      const h2 = await calls(page, 'mm_room_create');
      if (JSON.stringify(h2[1]) !== '{"mode":"ranked","public":false}') p.push(`Host a room after picking Ranked sent ${JSON.stringify(h2[1])}`);
      return p;
    },
  },
  'online-searching': {
    text: ['Looking for a ranked game', 'Searching… 0:42', 'Cancel'],
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
    text: ['Match found', 'VS', 'Ranked · first to two wins', 'Accept within 0:14', 'Jock', '#9 · 1,162 Elo', 'Adam'],
    absent: ['best of', 'Not on our servers?'],
    check: async (page) => {
      const p = [];
      for (const b of ['Accept', 'Decline', 'Leave match']) if (!(await page.getByRole('button', { name: b, exact: true }).count())) p.push(`no ${b} button`);
      // Ruling P8: Decline is the quiet button (spec §3.3).
      if (!(await page.locator('[data-key="decline"].btn.quiet').count())) p.push('Decline is not a quiet button');
      if (await page.locator('.on-vs .sweep').count()) p.push('the line sweeps before anyone accepted');
      if ((await page.locator('.on-vs img.team-logo, .on-vs .team-logo').count()) < 1) p.push("the opponent's team logo is missing");
      await page.getByRole('button', { name: 'Accept', exact: true }).click();
      await page.waitForTimeout(300);
      const a = await calls(page, 'mm_match_action');
      if (JSON.stringify(a[0]) !== '{"matchId":"m5","action":"accept","value":true}') p.push(`Accept sent ${JSON.stringify(a[0])}`);
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
    text: ['Your room', 'Casual', 'Room code', 'K7QP2M', 'Players', 'Adam', 'you', 'Ready', 'Not ready', 'Chat', 'ready when you are',
      "Start works once you're both ready. The host starts the game."],
    absent: ['Lobby code', 'Leave lobby'],
    check: async (page) => {
      const p = [];
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
      const p = [];
      if (!(await page.locator('.on-modes[inert]').count())) p.push('the tiles behind the panel can still be reached');
      await page.getByRole('button', { name: 'Sign in with Discord', exact: true }).click();
      await page.getByText('Your code is ABCD-1234', { exact: true }).waitFor({ timeout: 3000 }).catch(() => p.push('the device code is not shown'));
      if (!(await calls(page, 'identity_sign_in_start')).length) p.push('Sign in did not start the device-code flow');
      await page.locator('[data-key="signin-cancel"]').click();
      await page.getByRole('button', { name: 'Sign in with Discord', exact: true }).waitFor({ timeout: 3000 }).catch(() => p.push('Cancel did not return to the Sign in button'));
      return p;
    },
  },
  'online-offline': { text: ["Can't reach the Reborn servers right now.", 'Trying again.'], absent: ['Sign in with Discord', 'Find a', '0 online'] },
  'online-disabled': {
    text: ['Private room', 'Direct connection'],
    absent: ['Find a', "Can't reach", 'Sign in', 'Not on our servers', '0 online'],
    check: async (page) => {
      const p = [];
      if ((await page.locator('.on-tile').count()) !== 1) p.push(`${await page.locator('.on-tile').count()} tiles with matchmaking off`);
      await page.locator('.on-row [data-key="direct"]').click();
      if (!(await page.locator('#direct-dialog').evaluate((d) => d.open))) p.push('the Direct connection button did not open the dialog');
      return p;
    },
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
