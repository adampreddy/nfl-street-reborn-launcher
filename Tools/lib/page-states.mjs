// The screens the layout tools and the Stats/Profile checks open (plan 4): each page in the states that draw its
// layout, with answers shaped like the real commands, a frozen clock (2026-09-30 20:00 UTC, time zone UTC), no network
// (the backend's images and Discord avatars are answered with art/cover.png) and, on request, a connected controller.
//     import { STATES, answers, serve, openState, settle } from './lib/page-states.mjs';
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import * as F from '../fixtures/stats-fixtures.mjs';

export const root = path.resolve(import.meta.dirname, '..', '..');
export const ui = path.join(root, 'crates/reborn-ui/ui');
export const GAME_ID = 'aaaaaaaa-1111-4222-8333-444444444444';
const STAGES = ['welcome', 'game_verification', 'controller_setup', 'controls_profile', 'system_check', 'updates', 'preparation', 'completion'];
const SNAP = JSON.parse(readFileSync(path.join(root, 'Tools/fixtures/live-snapshot.json'), 'utf8'));
const LIST = JSON.parse(readFileSync(path.join(root, 'Tools/fixtures/live-list.json'), 'utf8'));
// The Online redesign's leaderboard and dashboard answers (spec 2026-10-02 §4, §7). adamknowsball is #7, as in the
// Stats fixture's elo(); week_delta is the backend's Task 15 field.
export const LB_TEAM = { tgid: 14, name: 'Falcons', abbrev: 'ATL', logo_url: '/live-assets/teams/14-128.png', primary: '#A71930' };
const LB_NAMES = ['Jock', 'Playz', 'Tuna', 'Mikel', 'Riley', 'Sauce', 'adamknowsball', 'Bones', 'Vick7', 'Kaz'];
const lbRow = (name, i) => ({ rank: i + 1, player_id: `00000000-0000-4000-8000-0000000000${String(i).padStart(2, '0')}`, display_name: name,
  avatar_url: i % 3 ? null : `https://cdn.discordapp.com/avatars/12345678901234567${i}/0123456789abcdef0123456789abcdef.png?size=64`,
  team: i % 2 ? null : LB_TEAM, wins: 20 - i, losses: 3 + i, elo: 1320 - i * 18, delta_week: i % 4 === 0 ? 16 : i % 4 === 1 ? -9 : 0, you: name === 'adamknowsball' });
export const LEADERBOARD = { ok: true, enabled: true, total: 38, rows: LB_NAMES.map(lbRow), you: null };
export const DASH = (() => { const d = F.dashboard(); d.you.elo.week_delta = 16; return d; })();

const SESSION = (id, ended, needs) => ({ id, build: 'offline', minutes: 3.2, ended, needs_recovery: needs });
const PADS = (n) => ({ pads: Array.from({ length: n }, (_, i) => ({ device_string: `XInput/${i}/Gamepad`, family: 'xbox', port: i + 1, set_aside: null })), other: [], complete: true, unsearched: [], source: 'builtin' });
const PREP = { summary: { controller: 'XInput/0/Gamepad', controller_port: 0, controls_profile: 'xbox', graphics: 'balanced', automatic_updates: true },
  preparation_remaining: [{ slug: 'write_config', label: 'Writing your settings' }, { slug: 'check_everything', label: 'Checking everything' }],
  tasks_done: [{ slug: 'verify_game', label: 'Checking your game' }] };
// A state may also carry: data (answers replacing the defaults), progress (extra onboarding_progress fields) and
// click (a selector clicked after load, to reach a tab).
export const STATES = {
  stats: [{ name: 'stats', url: 'stats.html', scope: 'main' }],
  profile: [{ name: 'profile', url: `profile.html?player=${F.ME}`, scope: 'main' }],
  recovery: [
    { name: 'recovery', url: 'recovery.html', scope: 'main' },
    { name: 'recovery-clean', url: 'recovery.html', scope: 'main', data: { sessions: [SESSION('s1', 'Exited', false)] } },
    { name: 'recovery-empty', url: 'recovery.html', scope: 'main', data: { sessions: [] } },
    { name: 'recovery-notready', url: 'recovery.html', scope: 'main', data: { launch_check: { ...answers().launch_check, ready: false, problem: 'The game file could not be found.' } } },
  ],
  game: [
    { name: 'game', url: `game.html?game=${GAME_ID}`, scope: 'main' },
    { name: 'game-plays', url: `game.html?game=${GAME_ID}`, scope: 'main', data: { stats_game: verifiedGame() } },
    { name: 'game-box', url: `game.html?game=${GAME_ID}`, scope: 'main', data: { stats_game: verifiedGame() }, click: '[data-key="tab-box"]' },
    { name: 'game-drives', url: `game.html?game=${GAME_ID}`, scope: 'main', data: { stats_game: verifiedGame() }, click: '[data-key="tab-drives"]' },
    { name: 'game-missing', url: 'game.html?game=nope', scope: 'main' },
  ],
  live: [{ name: 'live', url: `live.html?match=${SNAP.match.id}`, scope: 'main' }, { name: 'live-missing', url: 'live.html?match=nope', scope: 'main' }],
  online: [{ name: 'online', url: 'online.html', scope: 'main' }, ...onlineStates()],
  onboarding: [
    ...STAGES.map((s) => ({ name: `onboarding-${s}`, url: `onboarding.html?stage=${s}`, scope: 'page' })),
    { name: 'onboarding-preparation-tasks', url: 'onboarding.html?stage=preparation', scope: 'page', progress: PREP },
    { name: 'onboarding-controller-nopad', url: 'onboarding.html?stage=controller_setup', scope: 'page', data: { devices: PADS(0) } },
    { name: 'onboarding-controller-twopads', url: 'onboarding.html?stage=controller_setup', scope: 'page', data: { devices: PADS(2) } },
  ],
};

/** Online's matchmaking panel (online-mm.js reads `mm_state` and `mm_rooms`) and its private room (online.js polls
 *  `netplay_status`; an active snapshot shows the `.netplay-lobby` section), ruling P4-14. The answers have the shapes
 *  Tools/check-online-mm.mjs (captured from the real backend) and Tools/check-online-restored.mjs give them. The frozen
 *  clock keeps the queue timer still. */
function onlineStates() {
  const online = { count: 3, players: [{ name: 'Adam', activity: 'idle' }, { name: 'Jock', activity: 'queued' }, { name: 'Tuna', activity: 'idle' }] };
  const empty = { player: { name: 'Adam', region: 'na-east' }, ticket: null, room: null, match: null, chat: [], queue_depth: { ranked: 0, casual: 1 }, online };
  const mm = (snapshot, extra = {}) => ({ link: 'connected', error: null, presence_on: true, relay_error: null, snapshot, ...extra });
  const match = (m) => ({ id: 'm1', mode: 'casual', status: 'pairing', relay_state: 'pairing', relay_deadline: null, best_of: 1, you_host: true, opponent: 'Jock',
    field: null, your_field_pick: 'the_pit', your_accepted: false, your_ready: false, series: [0, 0], wins_needed: 1, winner_seat: null, hold_reason: null, seat: 0, ...m });
  const rooms = [{ code: 'K7QP2M', mode: 'casual', best_of: 1, host: 'Jock' }, { code: 'R4NK3D', mode: 'ranked', best_of: 3, host: 'Tuna' }];
  const players = [{ id: 1, name: 'Adam', status: 'OK', ping: '0 ms' }, { id: 2, name: 'Jock', status: 'OK', ping: '38 ms' }];
  const room = (r) => ({ netplay_status: { phase: 'lobby', active: true, host: true, connection: 'traversal', port: 2626, code: 'ABC12345', can_start: true, players, ports: [1, 2, 0, 0], ...r } });
  const S = (name, data) => ({ name, url: 'online.html', scope: 'main', data });
  // The redesign's states (spec 2026-10-02; Tools/check-online-page.mjs). PICK sets the stored mode before load.
  const PICK = (m) => `try{localStorage.setItem('mm:mode','${m}')}catch{}`;
  const N = (name, data, extra = {}) => ({ name, url: 'online.html', scope: 'main', data: { leaderboard: LEADERBOARD, stats_dashboard: DASH, mm_rooms: [], ...data }, ...extra });
  return [
    S('online-mm-idle', { mm_state: mm(empty), mm_rooms: rooms }),
    S('online-mm-queued', { mm_state: mm({ ...empty, ticket: { mode: 'casual', field: 'the_pit', waited_ms: 42000, enqueued_at: '2026-09-30T19:59:18.000Z' } }) }),
    S('online-mm-lobby', { mm_state: mm({ ...empty, room: { id: 'r1', code: 'K7QP2M', mode: 'casual', best_of: 1, status: 'open',
      members: [{ name: 'Jock', ready: true, field: 'city_limits', you: false }, { name: 'Adam', ready: false, field: 'the_pit', you: true }] },
      chat: [{ seq: 1, name: 'Jock', body: 'ready when you are', at: '2026-09-30T19:58:00Z' }] }) }),
    S('online-mm-pairing', { mm_state: mm({ ...empty, match: match({}) }) }),
    S('online-mm-match', { mm_state: mm({ ...empty, match: match({ id: 'm2', mode: 'ranked', status: 'running', relay_state: 'running', best_of: 3, you_host: false,
      your_accepted: true, your_ready: true, series: [0, 1], wins_needed: 2, seat: 1 }), chat: [{ seq: 1, name: 'Jock', body: 'gg', at: '2026-09-30T19:58:00Z' }] }) }),
    S('online-mm-relay-error', { mm_state: mm({ ...empty, match: match({ id: 'm4', relay_state: 'go', your_accepted: true, your_ready: true }) },
      { relay_error: 'This Dolphin build does not support launcher Netplay.' }) }),
    S('online-mm-held', { mm_state: mm({ ...empty, match: match({ id: 'm3', mode: 'ranked', status: 'held', relay_state: 'held', best_of: 3, your_accepted: true,
      your_ready: true, series: [1, 1], wins_needed: 2, hold_reason: 'content-mismatch' }) }), mm_rooms: [] }),
    S('online-mm-signed-out', { mm_state: mm(null, { link: 'signed_out' }) }),
    S('online-room-host', room({})),
    S('online-room-direct', room({ connection: 'direct', code: '', port: 2626 })),
    S('online-room-guest', room({ host: false, can_start: false })),
    S('online-room-running', room({ phase: 'running', can_start: false })),
    N('online-layout', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    N('online-small', { mm_state: mm(empty) }, { init: PICK('ranked'), viewport: { width: 800, height: 560 } }),
    // An invite link left a room code behind (online.js's takePendingInvite, answered at once): the Direct connection
    // dialog opens with the code filled (ruling P10: online-direct.js listens before online.js can fire).
    N('online-netplay-invite', { mm_state: mm(empty), netplay_pending_join: { code: 'K7QP2M9X' } }, { init: PICK('ranked') }),
    N('online-ranked', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    N('online-casual', { mm_state: mm(empty), content_repair_image: { current: true } }, { init: PICK('casual') }),
    N('online-private', { mm_state: mm(empty) }, { click: '[data-key="mode-private"]' }),
    N('online-searching', { mm_state: mm({ ...empty, ticket: { mode: 'ranked', field: null, waited_ms: 42000, enqueued_at: '2026-09-30T19:59:18.000Z' } }) }, { init: PICK('ranked') }),
    N('online-match-found', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, relay_deadline: '2026-09-30T20:00:14.000Z',
      opponent_elo: 1162, opponent_rank: 9, opponent_team: LB_TEAM, opponent_avatar_url: null }) }) }, { init: PICK('ranked') }),
    N('online-accepted', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, your_accepted: true, opponent_elo: 1162, opponent_rank: 9 }) }) }),
    N('online-starting', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, status: 'starting', relay_state: 'room_formed', your_accepted: true, your_ready: true }) }) }),
    N('online-playing', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, status: 'running', relay_state: 'running', your_accepted: true, your_ready: true, series: [1, 0] }),
      chat: [{ seq: 1, name: 'Jock', body: 'gg', at: '2026-09-30T19:58:00Z' }] }) }),
    // Ruling P9: between games of a series the card still says Playing, with the series line.
    N('online-intermission', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, status: 'running', relay_state: 'intermission', your_accepted: true, your_ready: true, series: [1, 0] }) }) }),
    N('online-playing-casual', { mm_state: mm({ ...empty, match: match({ id: 'm6', status: 'running', relay_state: 'running', your_accepted: true, your_ready: true }) }) }),
    N('online-match-bare', { mm_state: mm({ ...empty, match: match({ id: 'm7', opponent: null, best_of: null, series: null, relay_deadline: 'not a date', opponent_elo: null, opponent_rank: 0, opponent_team: null }) }) }),
    N('online-match-small', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, relay_deadline: '2026-09-30T20:00:14.000Z',
      opponent_elo: 1162, opponent_rank: 9, opponent_team: LB_TEAM }) }) }, { init: PICK('ranked'), viewport: { width: 800, height: 560 } }),
    N('online-room', { mm_state: mm({ ...empty, room: { id: 'r1', code: 'K7QP2M', mode: 'casual', best_of: 1, status: 'open', you_host: true,
      members: [{ name: 'Adam', ready: true, field: null, you: true }, { name: 'Jock', ready: false, field: null, you: false }] },
      chat: [{ seq: 1, name: 'Jock', body: 'ready when you are', at: '2026-09-30T19:58:00Z' }] }) },
      { init: "Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async(t)=>{window.__copied=t;}}})" }),
    N('online-held', { mm_state: mm({ ...empty, match: match({ id: 'm3', mode: 'ranked', status: 'held', relay_state: 'held', best_of: 3, your_accepted: true, your_ready: true,
      series: [1, 1], wins_needed: 2, hold_reason: 'content-mismatch' }) }) }),
    N('online-held-other', { mm_state: mm({ ...empty, match: match({ id: 'm8', status: 'held', relay_state: 'held', your_accepted: true, your_ready: true, hold_reason: 'series-too-fast' }) }) }),
    N('online-signed-out', { mm_state: mm(null, { link: 'signed_out' }),
      identity_sign_in_start: { device_code: 'dev-1', user_code: 'ABCD-1234', verify_url: 'https://discord.com/activate', interval: 5 } }, { init: PICK('ranked') }),
    N('online-connecting', { mm_state: mm(null) }),
    N('online-no-online-list', { mm_state: mm({ ...empty, online: undefined }) }, { init: PICK('ranked') }),
    N('online-offline', { mm_state: mm(null, { link: 'offline' }) }),
    N('online-disabled', { mm_state: mm(null, { link: 'disabled' }) }),
    N('online-presence', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    N('online-lobbies', { mm_state: mm(empty), mm_rooms: rooms }, { click: '[data-key="mode-private"]' }),
    N('online-lobbies-none', { mm_state: mm(empty), mm_rooms: [] }, { click: '[data-key="mode-private"]' }),
    N('online-live', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    N('online-live-many', { mm_state: mm(empty), live_matches: [0, 1, 2, 3, 4].map((i) => ({ ...LIST.matches[i % 2], id: `b41cf41a-43c4-4d1b-aaa8-05e4e72c73${String(i).padStart(2, '0')}` })) }, { init: PICK('ranked') }),
    N('online-live-none', { mm_state: mm(empty), live_matches: [] }, { init: PICK('ranked') }),
    // (redesign states are added above this line)
  ];
}

/** stats_game for a VERIFIED game (plays_status 'verified'), so game.js draws the tabs: plays, box score, drives. */
function verifiedGame() {
  const g = answers().stats_game;
  return { ...g, game: { ...g.game, plays_status: 'verified' }, plays: SNAP.games[1]?.plays ?? [] };
}

/** Every command's answer (check-shell-pages.mjs's FULL, plus what these pages read). Unknown commands answer null. */
export function answers() {
  const source = readFileSync(path.join(root, 'Tools/check-shell-pages.mjs'), 'utf8');
  const data = vm.runInNewContext(source.slice(source.indexOf('const FULL ='), source.indexOf('// Every field null')) + '\nFULL');
  const g0 = SNAP.games[0];
  const bindings = [...['A', 'B', 'X', 'Y', 'Z', 'Start'].map((x) => ({ key: `Buttons/${x}`, label: x, binding: `\`Button ${x}\`` })),
    ...['Up', 'Down', 'Left', 'Right'].flatMap((x) => ['Main Stick', 'C-Stick', 'D-Pad'].map((g) => ({ key: `${g}/${x}`, label: `${g} ${x}`, binding: `\`${x}\`` }))),
    ...['L', 'R'].map((x) => ({ key: `Triggers/${x}`, label: x, binding: `\`Trigger ${x}\`` }))];
  Object.assign(data, {
    session_active: false, ui_notices: [], netplay_status: null, netplay_pending_join: null, mm_state: null, mm_rooms: null, mm_launch_prefs: null,
    settings_paths: { runtime: null, game: null, userDir: null, legacyRoot: null },
    update_policy: { automatic: false, source: 'settings' }, update_check: { state: 'current', current: '3.0.0' }, update_activity: { downloading: false, ready: null },
    content_status: { phase: 'up_to_date', note: 'Everything is current.', components: [] },
    content_check: { phase: 'up_to_date', note: 'Everything is current.', components: [] },
    onboarding_progress: { complete: true, next: { kind: 'already_complete' }, plan: [], set_aside: [], summary: { graphics: 'balanced' } },
    launcher_look: { windowGlass: false, slowAnimations: false, glassSupported: true, glassApplied: false, glassNote: null },
    identity_status: { signed_in: true, display_name: 'adamknowsball', player_id: F.ME, show_offline: false },
    stats_queue: { ...data.stats_queue, depth: 2, entries: [{ key: 'q1', state: 'queued' }, { key: 'q2', state: 'stalled', reason: 'The server did not answer.' }] },
    live_matches: LIST.matches,
    live_match: SNAP,
    stats_game: { game: { id: GAME_ID, mode: 'online', participants: [], teams_view: g0.teams,
      teams: [{ name: g0.teams[0].name, abbrev: g0.teams[0].abbrev, score: g0.score[0] }, { name: g0.teams[1].name, abbrev: g0.teams[1].abbrev, score: g0.score[1] }] },
      plays: SNAP.games[1]?.plays ?? [], lines: g0.box.lines, drives: g0.drives },
    stats_dashboard: F.dashboard(),
    player_profile: F.profile(),
    player_profile_all: F.publicMatches(),
    setup_install_info: { mode: 'migrate', root: 'C:/Reborn', games: ['C:/Reborn/Games/NSR.iso'], settings_found: true, installed_release: { release: '2.1', game: 'week10', textures: 'week10' }, memory_cards: [] },
    setup_graphics_options: { current: null, previous: null },
    setup_hardware_info: { description: 'NVIDIA GeForce RTX 4070 · 32 GB RAM', recommended: 'quality' },
    setup_controller_mapping: { custom: false, rows: bindings },
  });
  return data;
}

const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.jpg': 'image/jpeg', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.json': 'application/json' };
export async function serve() {
  const server = createServer((req, res) => {
    let body = null;
    let p = '';
    try {
      p = path.resolve(ui, '.' + decodeURIComponent(new URL(req.url, 'http://x').pathname));
      // Read BEFORE writeHead: a missing file answers 404 instead of throwing after the 200 header went out (P4-10).
      if (p.startsWith(ui + path.sep)) body = readFileSync(p);
    } catch { body = null; }
    if (body === null) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'Content-Type': TYPES[path.extname(p)] || 'application/octet-stream' });
    res.end(body);
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  return { base: `http://127.0.0.1:${server.address().port}`, close: () => new Promise((r) => server.close(r)) };
}

/** Wait for every finite animation and transition on the page to finish before measuring (P4-8; copied from
 *  check-glass-pages.mjs). Infinite ones (the live pulse) are skipped and the wait never exceeds 5 s. The nav marker
 *  wait applies only where the page has one (the shell pages; onboarding has none). */
export async function settle(page) {
  await page.waitForFunction(() => { const m = document.querySelector('.nav-marker'); return !m || !m.classList.contains('still'); }, null, { timeout: 5000 }).catch(() => {});
  await page.evaluate(() => Promise.race([Promise.all(document.getAnimations().filter((a) => a.effect?.getComputedTiming().iterations !== Infinity).map((a) => a.finished.catch(() => {}))), new Promise((r) => setTimeout(r, 5000))]));
}

export async function openState(browser, base, state, { viewport = { width: 1280, height: 800 }, glass = false, slow = false, reducedMotion = 'reduce', pad = false, data = null, init = null } = {}) {
  const context = await browser.newContext({ viewport, reducedMotion, timezoneId: 'UTC', locale: 'en-US' });
  // `init`: an extra script run before the page's own (a check that drives the page's timers itself, say).
  if (init) await context.addInitScript(init);
  const png = readFileSync(path.join(ui, 'art/cover.png'));
  await context.route(/^https:\/\/(api\.nflstreetreborn\.com|cdn\.discordapp\.com)\//, (route) => route.fulfill({ status: 200, contentType: 'image/png', body: png }));
  const all = { ...answers(), ...(state.data || {}), ...(data || {}) };
  await context.addInitScript(({ data, stages, glass, slow, pad, extra }) => {
    const FIXED = Date.UTC(2026, 8, 30, 20, 0, 0);
    const RealDate = Date;
    class FixedDate extends RealDate { constructor(...a) { super(...(a.length ? a : [FIXED])); } static now() { return FIXED; } }
    window.Date = FixedDate;
    try { localStorage.setItem('reborn:look', JSON.stringify({ glass, slow })); } catch { /* private mode */ }
    const stage = new URL(location.href).searchParams.get('stage');
    if (stage) data.onboarding_progress = { complete: false, next: { kind: 'show', stage, position: stages.indexOf(stage) + 1, total: stages.length },
      plan: stages.map((s) => ({ stage: s, satisfied: stages.indexOf(s) < stages.indexOf(stage) })), set_aside: [], summary: {}, preparation_remaining: [], tasks_done: [], ...(extra || {}) };
    if (pad) {
      window.pads = [{ id: 'Xbox Wireless Controller', index: 0, connected: true, mapping: 'standard', axes: [0, 0, 0, 0], buttons: Array.from({ length: 17 }, () => ({ pressed: false, value: 0 })) }];
      Object.defineProperty(navigator, 'getGamepads', { configurable: true, value: () => window.pads });
      document.hasFocus = () => true;
    }
    window.__calls = [];
    window.__TAURI__ = { event: { listen: async () => () => {} }, core: { invoke: async (cmd, args = {}) => {
      window.__calls.push({ cmd, args });
      if (cmd === 'launcher_look') return { ...data.launcher_look, windowGlass: glass, slowAnimations: slow, glassApplied: glass };
      if (cmd === 'window_glass_apply') return { ...data.launcher_look, windowGlass: !!args.enabled, glassApplied: !!args.enabled };
      if (cmd === 'player_profile' && args.before) return { ok: true, matches: (data.player_profile_all || []).filter((m) => m.at < args.before).slice(0, 10) };
      if (cmd === 'launch') return new Promise(() => {});
      if (cmd in data) return data[cmd];
      if (/^onboarding_/.test(cmd)) return { outcome: 'recorded' };
      return null;
    } } };
  }, { data: JSON.parse(JSON.stringify(all)), stages: STAGES, glass, slow, pad, extra: state.progress || null });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(`${state.name}: ${e.message}`));
  await page.goto(`${base}/${state.url}`);
  await page.waitForFunction(() => { const s = document.getElementById('screen'); return s && !s.querySelector('.loading'); }, null, { timeout: 8000 }).catch(() => {});
  await page.evaluate(() => document.fonts.ready);
  if (state.click) await page.locator(state.click).first().click();
  await page.waitForTimeout(600);
  await settle(page);
  return { page, errors, context, close: () => context.close() };
}
