// The matchmaking chat keeps the reader's place (Adam 10-06: "would be nice in the launcher for text chat it didn't
// reset to the top every time someone types something requiring you to scroll down").
//
// Drives the Online page against a stubbed launcher bridge whose `mm_state` answer the test changes between draws
// (the page polls it every second), for a room chat and a match chat:
//
//   1. at the bottom, a new line arrives        -> the list follows it (still at the bottom)
//   2. scrolled up to read, a new line arrives  -> the list stays exactly where the reader put it
//   3. typing a reply (focused, text in the box) while a line arrives -> the line shows, the focus and the text stay
//   4. the chat list is the SAME element across every redraw (never rebuilt, which is what scrolled it to the top)
//
// Run: node Tools/check-chat-scroll.mjs [out-dir]   (screenshots land in target/browser-review)
import { chromium } from 'playwright';
import { createServer } from 'node:http';
import { readFileSync, mkdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const ui = path.join(root, 'crates/reborn-ui/ui');
const out = path.resolve(process.argv[2] || path.join(root, 'target/browser-review'));
mkdirSync(out, { recursive: true });

const lines = (n, from = 1) => Array.from({ length: n }, (_, i) => ({ seq: from + i, name: i % 2 ? 'Slick' : 'Jock', discord: i % 2 ? 'adam_discord' : null,
  text: `line ${from + i}: ${'some words to fill the row '.repeat(1 + (i % 3))}`, at: '2026-10-06T20:00:00Z' }));
const empty = { player: { name: 'Slick', discord: 'adam_discord', region: 'na-east' }, ticket: null, room: null, match: null, chat: [], queue_depth: { ranked: 0, casual: 0 }, online: { count: 2, players: [] } };
const SCOPES = {
  room: (chat) => ({ link: 'connected', error: null, presence_on: false, snapshot: { ...empty, chat,
    room: { id: 'r1', code: 'K7QP2M', mode: 'casual', best_of: 1, status: 'open', you_host: true,
      members: [{ name: 'Slick', discord: 'adam_discord', ready: false, you: true }, { name: 'Jock', ready: true, you: false }] } } }),
  match: (chat) => ({ link: 'connected', error: null, presence_on: false, snapshot: { ...empty, chat,
    match: { id: 'm1', mode: 'casual', status: 'running', relay_state: 'running', relay_deadline: null, best_of: 3, you_host: true, opponent: 'Jock',
      field: null, your_accepted: true, your_ready: true, series: [0, 0], wins_needed: 2, winner_seat: null, hold_reason: null, seat: 0 } } }),
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
const tick = 1300; // one draw (the page reads mm_state every second)
try {
  for (const [scope, make] of Object.entries(SCOPES)) {
    const problems = [];
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
    await context.addInitScript(({ first }) => {
      window.__state = first;
      window.__sent = [];
      window.__TAURI__ = { core: { invoke: async (cmd, args) => {
        if (cmd === 'mm_state') return window.__state;
        if (cmd === 'mm_room_action' || cmd === 'mm_match_action') { window.__sent.push(args); return {}; }
        if (cmd === 'netplay_status') return { active: false, phase: 'idle' };
        if (cmd === 'session_active') return false;
        return null;
      } } };
    }, { first: make(lines(20)) });
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e.message || e)));
    await page.goto(`${url}/online.html`);
    await page.locator('.mm-chat li').first().waitFor({ timeout: 5000 });
    await page.waitForTimeout(tick);
    const setChat = (chat) => page.evaluate(({ scope, chat }) => {
      // The same shape as `make`, rebuilt in the page: only the chat changes.
      const s = structuredClone(window.__state);
      s.snapshot.chat = chat;
      window.__state = s;
    }, { scope, chat });
    const geo = () => page.evaluate(() => {
      const l = document.querySelector('.mm-chat');
      if (!l.__mark) l.__mark = Math.random();
      return { top: l.scrollTop, gap: l.scrollHeight - l.scrollTop - l.clientHeight, overflow: l.scrollHeight > l.clientHeight, mark: l.__mark, count: l.children.length };
    });

    const g0 = await geo();
    if (!g0.overflow) problems.push('the chat does not overflow its box: the test proves nothing');
    if (g0.gap > 2) problems.push(`the first draw is not at the newest line (gap ${g0.gap}px)`);

    // 1. At the bottom: a new line arrives and the list follows it.
    await setChat(lines(21));
    await page.waitForTimeout(tick);
    const g1 = await geo();
    if (g1.count !== 21) problems.push(`after line 21: ${g1.count} lines drawn, want 21`);
    if (g1.gap > 2) problems.push(`at the bottom, a new line did not bring the list with it (gap ${g1.gap}px)`);
    if (g1.mark !== g0.mark) problems.push('the chat list was rebuilt on a redraw (a new element scrolls to the top)');

    // 2. Scrolled up to read: a new line arrives and the reader's place does not move.
    await page.evaluate(() => { document.querySelector('.mm-chat').scrollTop = 40; });
    await page.waitForTimeout(200);
    const before = (await geo()).top;
    await setChat(lines(22));
    await page.waitForTimeout(tick);
    const g2 = await geo();
    if (g2.count !== 22) problems.push(`after line 22: ${g2.count} lines drawn, want 22`);
    if (Math.abs(g2.top - before) > 1) problems.push(`scrolled up, a new line moved the reader: scrollTop ${before} -> ${g2.top}`);
    if (g2.top === 0) problems.push('scrolled up, a new line threw the reader back to the top');
    await page.locator('.on-chat').screenshot({ path: path.join(out, `chat-scroll-${scope}-reading.png`) });

    // 3. Typing a reply while a line arrives: the line shows, focus and text stay.
    await page.evaluate(() => { const l = document.querySelector('.mm-chat'); l.scrollTop = l.scrollHeight; });
    await page.locator('[data-key="chat"]').click();
    await page.keyboard.type('half a repl');
    await setChat(lines(23));
    await page.waitForTimeout(tick);
    const typing = await page.evaluate(() => ({ focused: document.activeElement?.getAttribute('data-key'), value: document.querySelector('[data-key="chat"]').value,
      count: document.querySelectorAll('.mm-chat li').length }));
    if (typing.count !== 23) problems.push(`while typing, the new line did not arrive (${typing.count} lines)`);
    if (typing.focused !== 'chat') problems.push(`while typing, the chat input lost focus (focus on ${typing.focused})`);
    if (typing.value !== 'half a repl') problems.push(`while typing, the text changed to "${typing.value}"`);
    await page.keyboard.type('y');
    await page.keyboard.press('Enter');
    await page.waitForTimeout(300);
    const sent = await page.evaluate(() => window.__sent.at(-1));
    if (sent?.value !== 'half a reply' || sent?.action !== 'chat') problems.push(`Enter did not send the reply: ${JSON.stringify(sent)}`);
    const g3 = await geo();
    if (g3.gap > 2) problems.push(`at the bottom while typing, the new line was not followed (gap ${g3.gap}px)`);

    // 4. More than the kept 30 lines: the oldest go, a reader scrolled up still does not move.
    await setChat(lines(40));
    await page.waitForTimeout(tick);
    await page.evaluate(() => { document.querySelector('.mm-chat').scrollTop = 120; });
    await page.waitForTimeout(200);
    const readAt = (await geo()).top;
    const firstSeen = await page.evaluate(() => document.querySelector('.mm-chat li:nth-child(5)').textContent);
    await setChat(lines(41));
    await page.waitForTimeout(tick);
    const g4 = await geo();
    if (g4.count !== 30) problems.push(`the list keeps ${g4.count} lines, want the last 30`);
    const stillSeen = await page.evaluate(({ firstSeen }) => {
      const l = document.querySelector('.mm-chat');
      const li = [...l.children].find((x) => x.textContent === firstSeen);
      return li ? li.offsetTop - l.scrollTop : null;
    }, { firstSeen });
    if (stillSeen === null) problems.push('the line the reader was on was dropped');
    else if (Math.abs(g4.top - readAt) > 200) problems.push(`trimming the oldest line moved the reader too far: ${readAt} -> ${g4.top}`);
    await page.locator('#main').screenshot({ path: path.join(out, `chat-scroll-${scope}.png`) });

    if (errors.length) problems.push(`page errors: ${errors.join(' | ')}`);
    if (problems.length) { failures++; console.log(`FAIL ${scope} chat\n   ${problems.join('\n   ')}`); }
    else console.log(`ok   ${scope} chat: follows at the bottom, stays put scrolled up, keeps the reply being typed`);
    await context.close();
  }
} finally {
  await browser.close();
  server.close();
}
if (failures) { console.log(`\n${failures} chat check(s) failed`); process.exit(1); }
console.log('\nchat scroll: all checks passed');
