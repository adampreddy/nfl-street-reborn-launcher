// Screenshots of the Online page's private room (the league game room, a friendly, a 2-player and a 4-player casual
// room) at the launcher's usual window and a small one. Screenshots land in target/room-look.
//     node Tools/shot-room.mjs
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import path from 'node:path';
import { root, serve, openState, settle } from './lib/page-states.mjs';

const out = path.join(root, 'target/room-look');
mkdirSync(out, { recursive: true });
const online = { count: 1, players: [{ name: 'Adam', activity: 'idle' }] };
const empty = { player: { name: 'Adam', region: 'na-east' }, ticket: null, room: null, match: null, chat: [], queue_depth: { ranked: 0, casual: 0 }, online };
const mm = (snapshot) => ({ link: 'connected', error: null, presence_on: true, relay_error: null, snapshot });
const soon = { ok: true, enabled: false, total: 0, rows: [], you: null };
const say = (seq, name, text) => ({ seq, name, text, at: `2026-09-30T19:5${seq}:00Z` });
const room = (r, chat = []) => ({ mm_state: mm({ ...empty, room: { id: 'r1', code: 'WHRF58', mode: 'casual', best_of: 1, status: 'open', you_host: true,
  members: [{ name: 'Adam', ready: false, field: null, you: true }], ...r }, chat }), leaderboard: soon, live_games: { ok: true, games: [] } });
const states = [
  ['league-alone', room({ edition: 'league', best_of: 3, league: { week: 1, fixture_id: 2614 } })],
  ['league-both', room({ edition: 'league', best_of: 3, league: { week: 1, fixture_id: 2614 },
    members: [{ name: 'Adam', ready: true, field: null, you: true }, { name: 'Jock', ready: false, field: null, you: false }] }, [say(1, 'Jock', 'gimme 2 min'), say(2, 'Adam', 'bet')])],
  ['friendly', room({ edition: 'league', ruleset: 'league' })],
  ['casual', room({ members: [{ name: 'Adam', ready: true, field: null, you: true }, { name: 'Jock', ready: true, field: null, you: false }] })],
  ['four', room({ capacity: 4, members: [{ name: 'Adam', ready: true, field: null, you: true }, { name: 'Jock', ready: true, field: null, you: false }, { name: 'Tuna', ready: false, field: null, you: false }] })],
];
const { base, close } = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
for (const [name, data] of states) {
  for (const [tag, viewport] of [['', { width: 1064, height: 775 }], ['-small', { width: 800, height: 560 }]]) {
    const s = { name, url: 'online.html', scope: 'main', data };
    const { page, errors, close: done } = await openState(browser, base, s, { viewport, glass: true });
    await settle(page);
    await page.waitForTimeout(250);
    await page.screenshot({ path: path.join(out, `${name}${tag}.png`) });
    const sideways = await page.evaluate(() => { const m = document.getElementById('main'); return m ? m.scrollWidth - m.clientWidth : 0; });
    console.log(`${errors.length || sideways > 1 ? 'FAIL' : 'ok  '} ${name}${tag}${errors.length ? ' ' + errors.join('; ') : ''}${sideways > 1 ? ` sideways ${sideways}px` : ''}`);
    await done();
  }
}
await browser.close();
await close();
