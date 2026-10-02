// Renders the Online page's matchmaking panel in every state it can be in, against a stubbed
// launcher bridge, and fails on a missing control or ANY page error.
//
// The snapshots below mirror what `online_mm::mm_state` returns, and their content (Adam vs Jock,
// casual, the_pit, a pairing relay) comes from responses captured from the real backend in
// crates/reborn-core/tests/fixtures/mm/. Screenshots of every state land in target/browser-review.
//
// Run: node Tools/check-online-mm.mjs
import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const ui = path.join(root, 'crates/reborn-ui/ui');
const out = path.resolve(process.argv[2] || path.join(root, 'target/browser-review'));
mkdirSync(out, { recursive: true });

const base = { link: 'connected', error: null, presence_on: true };
const online = { count: 3, players: [{ name: 'Adam', activity: 'idle' }, { name: 'Jock', activity: 'queued' }, { name: 'Tuna', activity: 'idle' }] };
const empty = { player: { name: 'Adam', region: 'na-east' }, ticket: null, room: null, match: null, chat: [], queue_depth: { ranked: 0, casual: 1 }, online };

const STATES = {
  signed_out: { ...base, link: 'signed_out', snapshot: null },
  offline: { ...base, link: 'offline', snapshot: null },
  disabled: { ...base, link: 'disabled', snapshot: null },
  idle: { ...base, snapshot: empty },
  // The same idle answer with the Private room tile picked (a page-state pick, clicked after load: CLICK below).
  idle_private: { ...base, snapshot: empty },
  queued: { ...base, snapshot: { ...empty, ticket: { mode: 'casual', field: 'the_pit', waited_ms: 42000, enqueued_at: '2026-09-22T20:34:50.379Z' } } },
  lobby: { ...base, snapshot: { ...empty, room: { id: 'r1', code: 'K7QP2M', mode: 'casual', best_of: 1, status: 'open', you_host: true,
    members: [{ name: 'Jock', ready: true, field: 'city_limits', you: false }, { name: 'Adam', ready: false, field: 'the_pit', you: true }] },
    chat: [{ seq: 1, name: 'Jock', body: 'ready when you are', at: '2026-09-22T20:40:00Z' }] } },
  pairing: { ...base, snapshot: { ...empty, match: { id: 'f772e15e-28e4-40ab-b667-70483f736542', mode: 'casual', status: 'pairing', relay_state: 'pairing',
    relay_deadline: '2026-09-22T20:35:10.425Z', best_of: 1, you_host: true, opponent: 'Jock', field: null, your_field_pick: 'the_pit',
    your_accepted: false, your_ready: false, series: [0, 0], wins_needed: 1, winner_seat: null, hold_reason: null, seat: 0 } } },
  running_bo3: { ...base, snapshot: { ...empty, match: { id: 'm2', mode: 'ranked', status: 'running', relay_state: 'running', relay_deadline: null,
    best_of: 3, you_host: false, opponent: 'Jock', field: 'City Limits', your_field_pick: 'the_pit', your_accepted: true, your_ready: true,
    series: [0, 1], wins_needed: 2, winner_seat: null, hold_reason: null, seat: 1 } } },
  relay_failed: { ...base, relay_error: 'This Dolphin build does not support launcher Netplay.', snapshot: { ...empty, match: { id: 'm4', mode: 'casual', status: 'pairing', relay_state: 'go',
    relay_deadline: null, best_of: 1, you_host: true, opponent: 'Jock', field: 'The Pit', your_field_pick: 'the_pit', your_accepted: true, your_ready: true,
    series: [0, 0], wins_needed: 1, winner_seat: null, hold_reason: null, seat: 0 } } },
  held: { ...base, snapshot: { ...empty, match: { id: 'm3', mode: 'ranked', status: 'held', relay_state: 'held', relay_deadline: null,
    best_of: 3, you_host: true, opponent: 'Jock', field: 'The Pit', your_field_pick: 'the_pit', your_accepted: true, your_ready: true,
    series: [1, 1], wins_needed: 2, winner_seat: null, hold_reason: 'series-too-fast', seat: 0 } } },
};

// A control clicked after the first draw, to reach a state the answer alone does not pick.
const CLICK = { idle_private: '[data-key="mode-private"]' };

// What each state must show. A control that is missing, or one that must NOT be offered, fails.
const EXPECT = {
  signed_out: { text: ['Sign in with Discord'], absent: ['Find a casual game'] },
  offline: { text: ["Can't reach the Reborn servers"], absent: ['Find a casual game'] },
  // Matchmaking switched off: the section is empty and the private-room page is untouched.
  disabled: { absent: ['Find a casual game', "Can't reach", 'Online play', 'Quick Match', 'Sign in'] },
  // The redesign (spec 2026-10-02 §3.1): tiles and an action row. Casual is the stored default; the lobby list and
  // the code field are under Private room (their assertions live in check-online-page's online-private/online-lobbies).
  idle: { text: ['Ranked', 'Casual', 'Private room', 'Find a casual game', 'Matched with anyone', '1 searching', '3 online'], buttons: ['Find a casual game'], absent: ['field pick', 'Quick Match'] },
  queued: { text: ['Looking for a casual game', 'Searching… 0:4'], buttons: ['Cancel'], absent: ['Find a casual game', 'field'] },
  // Ruling P15: the lobby list, joining by code and opening a public lobby stay asserted here, under Private room.
  idle_private: { text: ['Lobbies', 'Jock · Casual', 'Host a room'], buttons: ['Join', 'Join by code', 'Open a public lobby', 'Host a room'], absent: ['Find a casual game'] },
  // Ruling P14: the ready markers are counted (one ready, one not), not matched as loose text.
  lobby: { text: ['Your room', 'K7QP2M', 'Adam', 'you', 'Jock', 'Ready', 'Not ready', 'ready when you are'], buttons: ["I'm ready", 'Start', 'Leave room', 'Send', 'Copy code', 'Copy invite link'], absent: ['field pick'], counts: { '.on-rd': 1, '.on-nr': 1 } },
  pairing: { text: ['Jock', 'Match found', 'VS', 'Casual · one game'], buttons: ['Accept', 'Decline', 'Leave match'] },
  running_bo3: { text: ['Jock', 'Playing', 'Ranked · first to two wins', 'Game 2 · Series 1–0'], buttons: ['Leave match'], absent: ['Accept', 'City Limits', 'best of'] },
  relay_failed: { text: ['Jock', 'Getting ready', 'does not support launcher Netplay'], buttons: ['Leave match'] },
  held: { text: ['Held for review', 'Nothing has changed on the ladder'], absent: ['Leave match', 'Accept'] },
};

const types = { '.svg': 'image/svg+xml', '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.jpg': 'image/jpeg', '.woff2': 'font/woff2' };
const server = createServer((req, res) => {
  try {
    const p = decodeURIComponent(new URL(req.url, 'http://localhost').pathname);
    const file = path.resolve(ui, '.' + (p === '/' ? '/index.html' : p));
    if (!file.startsWith(ui + path.sep)) throw new Error('outside');
    res.writeHead(200, { 'Content-Type': types[path.extname(file)] || 'application/octet-stream' });
    res.end(readFileSync(file));
  } catch { res.writeHead(404); res.end('Not found'); }
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const url = `http://127.0.0.1:${server.address().port}`;
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });

let failures = 0;
try {
  for (const [name, state] of Object.entries(STATES)) {
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    await context.addInitScript(({ state }) => {
      window.__calls = [];
      const rooms = [{ code: 'K7QP2M', mode: 'casual', best_of: 1, host: 'Jock' }];
      window.__TAURI__ = { core: { invoke: async (cmd, args) => {
        window.__calls.push(cmd);
        if (cmd === 'mm_state') return state;
        if (cmd === 'mm_rooms') return rooms;
        if (cmd === 'netplay_status') return { active: false, phase: 'idle' };
        if (cmd === 'netplay_pending_join') return null;
        if (cmd === 'session_active') return false;
        return null;
      } } };
    }, { state });
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e.message || e)));
    await page.goto(`${url}/online.html`);
    await page.locator('section.mm').waitFor({ state: 'attached' });
    await page.waitForTimeout(1300); // one draw tick, plus the lobby list load
    if (CLICK[name]) { await page.locator(CLICK[name]).click({ timeout: 3000 }).catch(() => {}); await page.waitForTimeout(1300); }

    // Compared case-insensitively on purpose. innerText is the RENDERED text, and the page's CSS
    // uppercases headings, so "Casual lobby" arrives as "CASUAL LOBBY". An earlier version of this
    // checker compared exactly and failed five correct states - a check that fails on right output
    // is as useless as one that passes on wrong output.
    // The whole page body (#main): the online count sits in the heading since the redesign (spec 2026-10-02 §2).
    const text = (await page.locator('#main').innerText()).toLowerCase();
    const has = (t) => text.includes(t.toLowerCase());
    const exp = EXPECT[name];
    const problems = [];
    for (const t of exp.text ?? []) if (!has(t)) problems.push(`missing text "${t}"`);
    for (const b of exp.buttons ?? []) {
      if (!(await page.locator('#main').getByRole('button', { name: b, exact: true }).count())) problems.push(`missing button "${b}"`);
    }
    for (const [sel, n] of Object.entries(exp.counts ?? {})) {
      const got = await page.locator(sel).count();
      if (got !== n) problems.push(`${sel}: ${got} found, want ${n}`);
    }
    for (const t of exp.absent ?? []) if (has(t)) problems.push(`should NOT offer "${t}"`);
    if (errors.length) problems.push(`page errors: ${errors.join(' | ')}`);

    // The queue timer must COUNT between server updates (it froze once: the stub never changes, and
    // neither does a real snapshot for up to 25 s of long poll).
    if (name === 'queued') {
      const before = await page.locator('.mm-timer').innerText();
      await page.waitForTimeout(2200);
      const after = await page.locator('.mm-timer').innerText();
      if (before === after) problems.push(`the queue timer did not move: "${before}" stayed "${after}"`);
    }

    // The private-room section must be hidden exactly while matchmaking has the player busy, and
    // visible otherwise - including while a match is HELD, because a held match's seats are released.
    const busy = ['queued', 'lobby', 'pairing', 'running_bo3', 'relay_failed'].includes(name);
    const shown = await page.evaluate(() => getComputedStyle(document.getElementById('screen')).display !== 'none');
    if (busy && shown) problems.push('the private-room section is still offered while matchmaking has the player busy');
    if (!busy && !shown) problems.push('the private-room section is hidden when the player is free to use it');

    // Under Private room the public lobby opens with one click (mm_room_create; arguments are check-online-page's job).
    if (name === 'idle_private') {
      await page.locator('#main').getByRole('button', { name: 'Open a public lobby', exact: true }).click().catch(() => {});
      await page.waitForTimeout(300);
      if (!(await page.evaluate(() => window.__calls.includes('mm_room_create')))) problems.push('Open a public lobby did not send mm_room_create');
    }

    // The relay opens Dolphin by itself, so it must be told what Play would launch with.
    if (!(await page.evaluate(() => window.__calls.includes('mm_launch_prefs')))) problems.push('the page never reported its launch preferences to the relay');

    await page.locator('#main').screenshot({ path: path.join(out, `online-mm-${name}.png`) });
    if (problems.length) {
      failures++;
      console.log(`FAIL ${name}\n   ${problems.join('\n   ')}`);
    } else {
      console.log(`ok   ${name}`);
    }
    await context.close();
  }

  // Presence is told which page is open, on every page, through the shared shell.
  const context = await browser.newContext();
  await context.addInitScript(() => { window.__calls = []; window.__TAURI__ = { core: { invoke: async (cmd, a) => { window.__calls.push([cmd, a]); return null; } } }; });
  const page = await context.newPage();
  const errs = [];
  page.on('pageerror', (e) => errs.push(String(e.message || e)));
  await page.goto(`${url}/online.html`);
  await page.waitForTimeout(500);
  const sent = await page.evaluate(() => window.__calls.find(([c]) => c === 'presence_page'));
  if (!sent || sent[1]?.page !== 'online') { failures++; console.log(`FAIL presence page report: ${JSON.stringify(sent)}`); }
  else console.log('ok   presence is told the Online page is open');
  if (errs.length) { failures++; console.log(`FAIL a null-answering bridge broke the page: ${errs.join(' | ')}`); }
  else console.log('ok   a bridge that answers null leaves the panel out of the way, with no page error');
  await context.close();
} finally {
  await browser.close();
  await new Promise((r) => server.close(r));
}

assert.equal(failures, 0, `${failures} matchmaking state(s) failed`);
console.log(`PASS: ${Object.keys(STATES).length} matchmaking states render with the right controls and no page errors. Screenshots: ${out}`);
