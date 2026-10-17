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
// stats_game for a VERIFIED game: the backend's test/fixtures/site-restyle/644488b4-snapshot.json game 1 (Bills 8, Eagles
// 36) in the shape the server's /v1/games/:id, /plays, /boxscore and /drives give it (plays as { body, situation }).
export const GAME_644 = JSON.parse(readFileSync(path.join(root, 'Tools/fixtures/game-644488b4.json'), 'utf8'));
// A whole match on one page (owner 10-03): three games, oldest first. Game 1 the totals-only game (GAME_ID), game 2 the
// verified 644488b4, game 3 the same verified game under its own id.
export const GAME_3 = 'bbbbbbbb-2222-4333-8444-555555555555';
export function matchGames() {
  const third = structuredClone(GAME_644);
  third.game.id = GAME_3;
  return { [GAME_644.game.id]: GAME_644, [GAME_3]: third };
}
// The match page (match.html, the site's /live/<match>): a real ranked best of 3, GET /v1/live/5c8d58ca-... as the
// backend answered it on 2026-10-03 (49ers 2-1 Rams, Playz vs adamknowsball), as the live_match command hands it over.
export const MATCH_5C8 = JSON.parse(readFileSync(path.join(root, 'Tools/fixtures/match-5c8d58ca.json'), 'utf8'));
export const MATCH_PAGE_URL = `match.html?match=${MATCH_5C8.match.id}&from=stats.html`;
export const MATCH_URL = `match.html?game=${GAME_3}&games=${[GAME_ID, GAME_644.game.id, GAME_3].join(',')}&series=2-1&from=stats.html`;
// The Online redesign's leaderboard and dashboard answers (spec 2026-10-02 §4, §7). adamknowsball is #7, as in the
// Stats fixture's elo(); week_delta is the backend's Task 15 field. One player, one set of figures: the board's
// adamknowsball row is built from the same dashboard the Ranked tile reads (rating, rank, week change and the ranked
// record from elo.around), so tile, card and matchup card agree in every screenshot. The Online page reads your rank
// from this board's row (leaderboard.js), not from stats_dashboard.
export const LB_TEAM = { tgid: 14, name: 'Falcons', abbrev: 'ATL', logo_url: '/live-assets/teams/14-128.png', primary: '#A71930' };
export const DASH = (() => { const d = F.dashboard(); d.you.elo.week_delta = 16; return d; })();
const LB_NAMES = ['Jock', 'Playz', 'Tuna', 'Mikel', 'Riley', 'Sauce', 'adamknowsball', 'Bones', 'Vick7', 'Kaz'];
const LB_ELO = [1320, 1302, 1284, 1266, 1248, 1230, 1184, 1170, 1152, 1134];
const ME_ROW = (() => {
  const e = DASH.you.elo, a = e.around.find((r) => r.is_player);
  return { rank: e.rank, player_id: DASH.you.player_id, display_name: DASH.you.display_name, avatar_url: a.avatar_url, team: LB_TEAM,
    wins: a.wins, losses: a.losses, elo: e.rating, delta_week: e.week_delta, you: true };
})();
const lbRow = (name, i) => name === ME_ROW.display_name ? { ...ME_ROW } : ({ rank: i + 1, player_id: `00000000-0000-4000-8000-0000000000${String(i).padStart(2, '0')}`, display_name: name,
  avatar_url: i % 3 ? null : `https://cdn.discordapp.com/avatars/12345678901234567${i}/0123456789abcdef0123456789abcdef.png?size=64`,
  team: i % 2 ? null : LB_TEAM, wins: 20 - i, losses: 3 + i, elo: LB_ELO[i], delta_week: i % 4 === 0 ? 16 : i % 4 === 1 ? -9 : 0, you: false });
// A signed-out request carries no identity: the same board with no `you` flag and no pinned row (spec §4).
export const LEADERBOARD_ANON = (lb) => ({ ...lb, you: null, rows: lb.rows.map((r) => ({ ...r, you: false })) });
export const LEADERBOARD = { ok: true, enabled: true, total: 38, rows: LB_NAMES.map(lbRow), you: null };

const SESSION = (id, ended, needs) => ({ id, build: 'offline', minutes: 3.2, ended, needs_recovery: needs });
const PADS = (n) => ({ pads: Array.from({ length: n }, (_, i) => ({ device_string: `XInput/${i}/Gamepad`, family: 'xbox', port: i + 1, set_aside: null })), other: [], complete: true, unsearched: [], source: 'builtin' });
const PREP = { summary: { controller: 'XInput/0/Gamepad', controller_port: 0, controls_profile: 'xbox', graphics: 'balanced', automatic_updates: true },
  preparation_remaining: [{ slug: 'write_config', label: 'Writing your settings' }, { slug: 'check_everything', label: 'Checking everything' }],
  tasks_done: [{ slug: 'verify_game', label: 'Checking your game' }] };
// A state may also carry: data (answers replacing the defaults), progress (extra onboarding_progress fields) and
// click (a selector clicked after load, to reach a tab).
export const STATES = {
  stats: [{ name: 'stats', url: 'stats.html', scope: 'main' }, { name: 'stats-records', url: 'stats.html#records', scope: 'main' }],
  // The profile (mv2, no tabs), then each fuller view by its address.
  profile: [{ name: 'profile', url: `profile.html?player=${F.ME}`, scope: 'main' },
    ...['matches', 'stats', 'records', 'h2h'].map((t) => ({ name: `profile-${t}`, url: `profile.html?player=${F.ME}#${t}`, scope: 'main' }))],
  recovery: [
    { name: 'recovery', url: 'recovery.html', scope: 'main' },
    { name: 'recovery-clean', url: 'recovery.html', scope: 'main', data: { sessions: [SESSION('s1', 'Exited', false)] } },
    { name: 'recovery-empty', url: 'recovery.html', scope: 'main', data: { sessions: [] } },
    { name: 'recovery-notready', url: 'recovery.html', scope: 'main', data: { launch_check: { ...answers().launch_check, ready: false, problem: 'The game file could not be found.' } } },
  ],
  game: [
    // A game (no match id) opens the match page by its games (owner 10-03: one style for every box score): an unverified
    // game shows the strip and the totals-only words; a verified one (GAME_644, the site's own 644488b4 fixture as
    // stats_game answers it) its tabs; a match of several games every game of it.
    { name: 'game', url: `match.html?game=${GAME_ID}`, scope: 'main' },
    { name: 'game-feed', url: `match.html?game=${GAME_644.game.id}`, scope: 'main', data: { stats_game: GAME_644 } },
    { name: 'game-box', url: `match.html?game=${GAME_644.game.id}`, scope: 'main', data: { stats_game: GAME_644 }, click: '[data-key="tab-box"]' },
    { name: 'game-team', url: `match.html?game=${GAME_644.game.id}`, scope: 'main', data: { stats_game: GAME_644 }, click: '[data-key="tab-team"]' },
    { name: 'game-missing', url: 'match.html?game=nope', scope: 'main' },
    { name: 'game-match', url: MATCH_URL, scope: 'main', data: { stats_game_by_id: matchGames() } },
  ],
  // The match page (a matchmade match): the site's /live/<match> page on the real 5c8d58ca snapshot, each tab.
  match: [
    { name: 'match-feed', url: MATCH_PAGE_URL, scope: 'main', data: { live_match: MATCH_5C8 } },
    { name: 'match-box', url: MATCH_PAGE_URL, scope: 'main', data: { live_match: MATCH_5C8 }, click: '[data-key="tab-box"]' },
    { name: 'match-team', url: MATCH_PAGE_URL, scope: 'main', data: { live_match: MATCH_5C8 }, click: '[data-key="tab-team"]' },
    { name: 'match-missing', url: 'match.html?match=nope', scope: 'main' },
  ],
  // A live game from Online's Live now (owner 10-08): the match page (the old live.html view is retired), Back to Online.
  live: [{ name: 'live-feed', url: `match.html?match=${MATCH_5C8.match.id}&from=online.html`, scope: 'main', data: { live_match: MATCH_5C8 } }],
  // The full leaderboard (owner 10-08), from Online's "Full leaderboard ›". Tools/check-live-feed.mjs pages it.
  leaderboard: [{ name: 'leaderboard', url: 'leaderboard.html', scope: 'main', data: { leaderboard: { ...LEADERBOARD, offset: 0 } } },
    { name: 'leaderboard-soon', url: 'leaderboard.html', scope: 'main', data: { leaderboard: { ok: true, enabled: false, total: 0, rows: [], you: null } } }],
  online: [{ name: 'online', url: 'online.html', scope: 'main' }, ...onlineStates()],
  league: leagueStates(),
  onboarding: [
    ...STAGES.map((s) => ({ name: `onboarding-${s}`, url: `onboarding.html?stage=${s}`, scope: 'page' })),
    { name: 'onboarding-preparation-tasks', url: 'onboarding.html?stage=preparation', scope: 'page', progress: PREP },
    { name: 'onboarding-controller-nopad', url: 'onboarding.html?stage=controller_setup', scope: 'page', data: { devices: PADS(0) } },
    { name: 'onboarding-controller-twopads', url: 'onboarding.html?stage=controller_setup', scope: 'page', data: { devices: PADS(2) } },
  ],
};

/** Online's matchmaking panel (online-mm.js reads `mm_state`; public lobbies and `mm_rooms` went 2026-10-03) and its private room (online.js polls
 *  `netplay_status`; an active snapshot shows the `.netplay-lobby` section), ruling P4-14. The answers have the shapes
 *  Tools/check-online-mm.mjs (captured from the real backend) and Tools/check-online-restored.mjs give them. The frozen
 *  clock keeps the queue timer still. */
function onlineStates() {
  const online = { count: 3, players: [{ name: 'Adam', activity: 'idle' }, { name: 'Jock', activity: 'queued' }, { name: 'Tuna', activity: 'idle' }] };
  const empty = { player: { name: 'Adam', region: 'na-east' }, ticket: null, room: null, match: null, chat: [], queue_depth: { ranked: 0, casual: 1 }, online };
  const mm = (snapshot, extra = {}) => ({ link: 'connected', error: null, presence_on: true, relay_error: null, snapshot, ...extra });
  const match = (m) => ({ id: 'm1', mode: 'casual', status: 'pairing', relay_state: 'pairing', relay_deadline: null, best_of: 1, you_host: true, opponent: 'Jock',
    field: null, your_field_pick: 'the_pit', your_accepted: false, your_ready: false, series: [0, 0], wins_needed: 1, winner_seat: null, hold_reason: null, seat: 0, ...m });
  const players = [{ id: 1, name: 'Adam', status: 'OK', ping: '0 ms' }, { id: 2, name: 'Jock', status: 'OK', ping: '38 ms' }];
  const room = (r) => ({ netplay_status: { phase: 'lobby', active: true, host: true, connection: 'traversal', port: 2626, code: 'ABC12345', can_start: true, players, ports: [1, 2, 0, 0], ...r } });
  const S = (name, data) => ({ name, url: 'online.html', scope: 'main', data });
  // The redesign's states (spec 2026-10-02; Tools/check-online-page.mjs). PICK sets the stored mode before load.
  const PICK = (m) => `try{localStorage.setItem('mm:mode','${m}')}catch{}`;
  const MATCH_FOUND = { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, relay_deadline: '2026-09-30T20:00:14.000Z',
    opponent_elo: 1320, opponent_rank: 1, opponent_team: LB_TEAM, opponent_avatar_url: null }) }) };
  const N = (name, data, extra = {}) => ({ name, url: 'online.html', scope: 'main', data: { leaderboard: LEADERBOARD, ...data }, ...extra });
  // Chat lines as the launcher hands them to the page: the line as `text` (the server's name; mm_state carries it under
  // `body` too). online-mm-lobby keeps a `body`-only line, the page's older shape.
  const say = (seq, name, text) => ({ seq, name, text, at: `2026-09-30T19:5${seq}:00Z` });
  const ROOM = (r, chat = []) => ({ ...empty, room: { id: 'r1', code: 'K7QP2M', mode: 'casual', best_of: 1, status: 'open', you_host: true,
    members: [{ name: 'Adam', ready: true, field: null, you: true }, { name: 'Jock', ready: false, field: null, you: false }], ...r }, chat });
  // Your public profile: the team you play most online (Chargers) differs from the one you picked to play as (Falcons).
  const PROFILE_MOST = { ...F.profile(), header: { ...F.profile().header, most_used_team: { name: 'Chargers', abbrev: 'LAC', tgid: 27, logo_url: null, primary: '#0080c6', secondary: '#ffc20e' } } };
  return [
    S('online-mm-idle', { mm_state: mm(empty) }),
    S('online-mm-queued', { mm_state: mm({ ...empty, ticket: { mode: 'casual', field: 'the_pit', waited_ms: 42000, enqueued_at: '2026-09-30T19:59:18.000Z' } }) }),
    S('online-mm-lobby', { mm_state: mm({ ...empty, room: { id: 'r1', code: 'K7QP2M', mode: 'casual', best_of: 1, status: 'open',
      members: [{ name: 'Jock', ready: true, field: 'city_limits', you: false }, { name: 'Adam', ready: false, field: 'the_pit', you: true }] },
      chat: [{ seq: 1, name: 'Jock', body: 'ready when you are', at: '2026-09-30T19:58:00Z' }] }) }),
    S('online-mm-pairing', { mm_state: mm({ ...empty, match: match({}) }) }),
    S('online-mm-match', { mm_state: mm({ ...empty, match: match({ id: 'm2', mode: 'ranked', status: 'running', relay_state: 'running', best_of: 3, you_host: false,
      your_accepted: true, your_ready: true, series: [0, 1], wins_needed: 2, seat: 1 }), chat: [say(1, 'Jock', 'gg')] }) }),
    S('online-mm-relay-error', { mm_state: mm({ ...empty, match: match({ id: 'm4', relay_state: 'go', your_accepted: true, your_ready: true }) },
      { relay_error: 'This Dolphin build does not support launcher Netplay.' }) }),
    S('online-mm-held', { mm_state: mm({ ...empty, match: match({ id: 'm3', mode: 'ranked', status: 'held', relay_state: 'held', best_of: 3, your_accepted: true,
      your_ready: true, series: [1, 1], wins_needed: 2, hold_reason: 'content-mismatch' }) }) }),
    S('online-mm-signed-out', { mm_state: mm(null, { link: 'signed_out' }) }),
    S('online-room-host', room({})),
    S('online-room-direct', room({ connection: 'direct', code: '', port: 2626 })),
    S('online-room-guest', room({ host: false, can_start: false })),
    // 10-02: matchmaking's own Netplay room, with data-mm-busy not (yet) set: the Direct connection dialog stays shut.
    S('online-room-matchmade', room({ host: false, can_start: false, matchmade: true })),
    S('online-room-running', room({ phase: 'running', can_start: false })),
    N('online-layout', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    N('online-small', { mm_state: mm(empty) }, { init: PICK('ranked'), viewport: { width: 800, height: 560 } }),
    // An invite link left a room code behind (online.js's takePendingInvite, answered at once): the Direct connection
    // dialog opens with the code filled (ruling P10: online-direct.js listens before online.js can fire).
    N('online-netplay-invite', { mm_state: mm(empty), netplay_pending_join: { code: 'K7QP2M9X' } }, { init: PICK('ranked') }),
    N('online-ranked', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    N('online-casual', { mm_state: mm(empty), content_repair_image: { current: true } }, { init: PICK('casual') }),
    // The ranked rules (Adam, 2026-10-03): shown before the first ranked queue, and from "Ranked rules ›".
    N('online-ranked-rules', { mm_state: mm(empty), content_repair_image: { current: true } }, { init: PICK('ranked') }),
    N('online-private', { mm_state: mm(empty) }, { click: '[data-key="mode-private"]' }),
    N('online-searching', { mm_state: mm({ ...empty, ticket: { mode: 'ranked', field: null, waited_ms: 42000, enqueued_at: '2026-09-30T19:59:18.000Z' } }) }, { init: PICK('ranked') }),
    // A casual Pickup search, with the team you play most (most_used_team, not the plays-as pick).
    N('online-searching-casual', { mm_state: mm({ ...empty, ticket: { mode: 'casual', field: null, seed_kind: 'pickup', waited_ms: 42000, enqueued_at: '2026-09-30T19:59:18.000Z' } }),
      player_profile: PROFILE_MOST }, { init: PICK('casual') }),
    N('online-match-found', MATCH_FOUND, { init: PICK('ranked') }),
    // The same, with the check moving the page's clock: "Accept within" counts down every second and hides at the deadline.
    N('online-match-found-tick', MATCH_FOUND, { init: PICK('ranked') }),
    N('online-accepted', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, your_accepted: true, opponent_elo: 1320, opponent_rank: 1 }) }) }),
    N('online-starting', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, status: 'starting', relay_state: 'room_formed', your_accepted: true, your_ready: true }) }) }),
    N('online-playing', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, status: 'running', relay_state: 'running', your_accepted: true, your_ready: true, series: [1, 0] }),
      chat: [say(1, 'Jock', 'gg')] }) }),
    // Ruling P9: between games of a series the card still says In play, with the series line.
    N('online-intermission', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, status: 'running', relay_state: 'intermission', your_accepted: true, your_ready: true, series: [1, 0] }) }) }),
    N('online-playing-casual', { mm_state: mm({ ...empty, match: match({ id: 'm6', status: 'running', relay_state: 'running', your_accepted: true, your_ready: true }) }) }),
    N('online-match-bare', { mm_state: mm({ ...empty, match: match({ id: 'm7', opponent: null, best_of: null, series: null, relay_deadline: 'not a date', opponent_elo: null, opponent_rank: 0, opponent_team: null }) }) }),
    N('online-match-small', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2, relay_deadline: '2026-09-30T20:00:14.000Z',
      opponent_elo: 1320, opponent_rank: 1, opponent_team: LB_TEAM }) }) }, { init: PICK('ranked'), viewport: { width: 800, height: 560 } }),
    N('online-room', { mm_state: mm(ROOM({}, [say(1, 'Jock', 'ready when you are')])) },
      { init: "Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async(t)=>{window.__copied=t;}}})" }),
    // The host switched the room to Pickup (the snapshot's seed_kind); a GUEST reads it in the title, with no switch.
    N('online-room-pickup', { mm_state: mm({ ...empty, room: { id: 'r1', code: 'K7QP2M', mode: 'casual', best_of: 1, status: 'open', you_host: false, seed_kind: 'pickup',
      members: [{ name: 'Jock', ready: true, field: null, you: false }, { name: 'Adam', ready: false, field: null, you: true }] }, chat: [] }) }),
    // Today's server has no kind route: the switch is refused with a quiet sentence and the room carries on.
    N('online-room-kind-old', { mm_state: mm(ROOM({})), mm_room_action: { __reject: 'Not found' } }),
    // The game is starting: the host's switch is shown but cannot be used.
    N('online-room-started', { mm_state: mm(ROOM({ status: 'starting', members: [{ name: 'Adam', ready: true, field: null, you: true }, { name: 'Jock', ready: true, field: null, you: false }] })) }),
    // 2v2 private rooms (Adam 10-04): a room this launcher opened takes four. Three in, and four in at the minimum window.
    N('online-room-three', { mm_state: mm(ROOM({ capacity: 4, members: [{ name: 'Adam', ready: true, field: null, you: true },
      { name: 'Jock', ready: true, field: null, you: false }, { name: 'Tuna', ready: false, field: null, you: false }] })) }),
    N('online-room-four-small', { mm_state: mm(ROOM({ capacity: 4, you_host: false, members: [{ name: 'Adam', ready: true, field: null, you: false },
      { name: 'Jock', ready: true, field: null, you: false }, { name: 'Tuna', ready: true, field: null, you: true }, { name: 'Mack', ready: false, field: null, you: false }] },
      [say(1, 'Mack', 'me and tuna vs you two')])) }, { viewport: { width: 800, height: 560 } }),
    // A four-player match as its third player sees it: the host's face, and everyone else under the VS.
    N('online-match-four', { mm_state: mm({ ...empty, match: match({ id: 'm9', status: 'starting', relay_state: 'joining', your_accepted: true, your_ready: true,
      opponent: 'Adam', seat: 2, you_host: false, party: [{ seat: 0, name: 'Adam', you: false }, { seat: 1, name: 'Jock', you: false },
        { seat: 2, name: 'Tuna', you: true }, { seat: 3, name: 'Mack', you: false }] }) }) }),
    // A room with a conversation (the chat keeps every line, 2026-10-03).
    N('online-room-chat', { mm_state: mm(ROOM({}, [say(1, 'Jock', 'yo, you up?'), say(2, 'Adam', 'running it back'), say(3, 'Jock', 'Pickup this time'),
      say(4, 'Adam', 'bet. switching it'), say(5, 'Jock', 'ready when you are')])) }),
    N('online-held', { mm_state: mm({ ...empty, match: match({ id: 'm3', mode: 'ranked', status: 'held', relay_state: 'held', best_of: 3, your_accepted: true, your_ready: true,
      series: [1, 1], wins_needed: 2, hold_reason: 'content-mismatch' }) }) }),
    N('online-held-other', { mm_state: mm({ ...empty, match: match({ id: 'm8', status: 'held', relay_state: 'held', your_accepted: true, your_ready: true, hold_reason: 'series-too-fast' }) }) }),
    N('online-signed-out', { leaderboard: LEADERBOARD_ANON(LEADERBOARD), mm_state: mm(null, { link: 'signed_out' }),
      identity_sign_in_start: { device_code: 'dev-1', user_code: 'ABCD-1234', verify_url: 'https://discord.com/activate', interval: 5 } }, { init: PICK('ranked') }),
    N('online-connecting', { mm_state: mm(null) }),
    N('online-no-online-list', { mm_state: mm({ ...empty, online: undefined }) }, { init: PICK('ranked') }),
    N('online-offline', { mm_state: mm(null, { link: 'offline' }) }),
    N('online-disabled', { mm_state: mm(null, { link: 'disabled' }) }),
    N('online-presence', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    // No public lobbies (Adam, 2026-10-03: "you just have to send the codes"): nothing lists or opens one.
    N('online-no-lobbies', { mm_state: mm(empty) }, { click: '[data-key="mode-private"]' }),
    N('online-live', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    N('online-live-many', { mm_state: mm(empty), live_matches: [0, 1, 2, 3, 4].map((i) => ({ ...LIST.matches[i % 2], id: `b41cf41a-43c4-4d1b-aaa8-05e4e72c73${String(i).padStart(2, '0')}` })) }, { init: PICK('ranked') }),
    N('online-live-none', { mm_state: mm(empty), live_matches: [] }, { init: PICK('ranked') }),
    N('online-leaderboard', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    // Further down the board: #14 at 1,104 on both the card and the Ranked tile; someone else (Dime) holds #7.
    N('online-leaderboard-far', { mm_state: mm(empty), leaderboard: { ...LEADERBOARD,
      rows: LEADERBOARD.rows.map((r) => (r.you ? { ...r, player_id: '00000000-0000-4000-8000-000000000006', display_name: 'Dime', avatar_url: null, wins: 12, losses: 6, delta_week: 0, you: false } : r)),
      you: { ...ME_ROW, rank: 14, elo: 1104 } } }, { init: PICK('ranked') }),
    // A leaderboard read that fails after a good one (a network blip): the check re-polls with the read rejected.
    N('online-rank-blip', { mm_state: mm(empty) }, { init: PICK('ranked') }),
    // Elo off: no row of yours, so the plays-as team comes from one player_profile read (F.profile(): Falcons).
    N('online-elo-off', { mm_state: mm(empty), leaderboard: { ok: true, enabled: false, total: 0, rows: [], you: null } }, { init: PICK('ranked') }),
    N('online-lb-missing', { mm_state: mm(empty), leaderboard: null }, { init: PICK('ranked') }),
    N('online-lb-hostile', { mm_state: mm(empty), leaderboard: { ok: true, enabled: true, total: 2, you: null, rows: [
      { rank: 1, player_id: 'h1', display_name: '<img src=x onerror=alert(1)>', avatar_url: 'https://evil.example/a.png', team: { name: 'X', abbrev: 'X', logo_url: 'https://evil.example/x.png' }, wins: 3, losses: 1, elo: 1200, delta_week: 0, you: false },
      { rank: 2, player_id: 'h2', display_name: 'Averyveryveryveryveryverylongdisplayname1234567890', avatar_url: null, team: null, wins: null, losses: null, elo: null, delta_week: null, you: false },
    ] } }, { init: PICK('ranked') }),
    // A room invite link (spec §3.10, Task 12): the page offers Join, never joins on its own, and keeps the invite while
    // signed out or queued (no banner while busy).
    N('online-invite', { mm_state: mm(empty), mm_pending_room: { kind: 'room', code: 'K7QP2M', at: '2026-09-30T19:59:00Z' }, mm_room_lookup: { host_name: 'Jock', mode: 'casual' } }),
    N('online-invite-nohost', { mm_state: mm(empty), mm_pending_room: { kind: 'room', code: 'K7QP2M', at: '2026-09-30T19:59:00Z' }, mm_room_lookup: null }),
    N('online-invite-signed-out', { leaderboard: LEADERBOARD_ANON(LEADERBOARD), mm_state: mm(null, { link: 'signed_out' }), mm_pending_room: { kind: 'room', code: 'K7QP2M', at: '2026-09-30T19:59:00Z' } }),
    N('online-invite-not-now', { leaderboard: LEADERBOARD_ANON(LEADERBOARD), mm_state: mm(null, { link: 'signed_out' }), mm_pending_room: { kind: 'room', code: 'K7QP2M', at: '2026-09-30T19:59:00Z' } }),
    N('online-invite-long', { mm_state: mm(empty), mm_pending_room: { kind: 'room', code: 'K7QP2M9X12345', at: '2026-09-30T19:59:00Z' }, mm_room_lookup: { host_name: 'Jock', mode: 'casual' } }),
    N('online-invite-busy', { mm_state: mm({ ...empty, ticket: { mode: 'casual', field: null, waited_ms: 5000, enqueued_at: '2026-09-30T19:59:55.000Z' } }),
      mm_pending_room: { kind: 'room', code: 'K7QP2M', at: '2026-09-30T19:59:00Z' } }),
    // A Netplay invite while queued (carried from Task 2): the Direct connection dialog stays shut.
    N('online-netplay-invite-busy', { mm_state: mm({ ...empty, ticket: { mode: 'casual', field: null, waited_ms: 5000, enqueued_at: '2026-09-30T19:59:55.000Z' } }),
      netplay_pending_join: { code: 'K7QP2M9X' } }),
    N('online-pad', { mm_state: mm(empty) }, { init: PICK('ranked'), pad: true }),
    N('online-searching-esc', { mm_state: mm({ ...empty, ticket: { mode: 'ranked', field: null, waited_ms: 42000, enqueued_at: '2026-09-30T19:59:18.000Z' } }) }, { init: PICK('ranked') }),
    N('online-searching-panel-esc', { mm_state: mm({ ...empty, ticket: { mode: 'ranked', field: null, waited_ms: 42000, enqueued_at: '2026-09-30T19:59:18.000Z' } }) }, { init: PICK('ranked') }),
    N('online-match-found-esc', { mm_state: mm({ ...empty, match: match({ id: 'm5', mode: 'ranked', best_of: 3, wins_needed: 2 }) }) }),
    N('online-focus-keep', { mm_state: mm(empty) }, { click: '[data-key="mode-private"]' }),
    // The chat field keeps its text and focus across redraws too (ruling P16): a room with chat.
    N('online-focus-keep-chat', { mm_state: mm({ ...empty, room: { id: 'r1', code: 'K7QP2M', mode: 'casual', best_of: 1, status: 'open', you_host: true,
      members: [{ name: 'Adam', ready: true, field: null, you: true }, { name: 'Jock', ready: false, field: null, you: false }] },
      chat: [say(1, 'Jock', 'ready when you are')] }) }),
    // The post-game screen (series over): a real ranked best of 3 (MATCH_5C8, 49ers 2-1 Rams) as the live snapshot, the
    // finished match in mm_state. OVER_ME puts adamknowsball (you) in seat 0, the 49ers' seat, so you won it.
    ...overStates(N, mm, empty, match, PICK),
    // (redesign states are added above this line)
  ];
}

/** The League tab (plan 3a Task 5) as a member sees it: Tools/fixtures/league-home.json filled out to a 12-team week
 *  (six games, the full table, five results, three leaders each, a roster), then each This week state, the League
 *  files states and an outage. No logos (letters on team colours): the fake image route would draw the cover art. */
function leagueHome() {
  const h = JSON.parse(readFileSync(path.join(root, 'Tools/fixtures/league-home.json'), 'utf8'));
  const T = (team_id, name, abbr, color, record) => ({ team_id, name, abbr, logo_url: null, color, record });
  const me = T(493, 'Subway Surfers', 'ADAM', '#00A3FF', '4-1'), opp = T(486, 'Football Furies', 'BLONDE', '#FF0000', '3-2');
  const g = (fixture_id, away, home, status, extra = {}) => ({ fixture_id, away, home, status, forfeit: false, score: null, series: null, winner_team_id: null, when: null, ...extra });
  h.team.logo_url = null;
  h.matchup.opponent.logo_url = null;
  h.week_games = [
    g(1234, opp, me, 'scheduled'),
    g(1230, T(483, 'Jock Squad', 'JOCK', '#AA2222', '2-3'), T(482, 'Gridiron Ghosts', 'GHO', '#22AA44', '5-0'), 'final', { score: { home: 49, away: 21 }, series: { home: 2, away: 0 }, winner_team_id: 482 }),
    g(1231, T(485, 'Sand Crabs', 'CRAB', '#DDAA00', '1-4'), T(484, 'Night Owls', 'OWL', '#6A5ACD', '2-3'), 'live', { score: { home: 14, away: 21 }, series: { home: 0, away: 1 } }),
    g(1232, T(488, 'Dune Kings', 'DUNE', '#AA7700', '2-3'), T(487, 'Rail Riders', 'RAIL', '#5555FF', '3-2'), 'final', { forfeit: true, winner_team_id: 487 }),
    g(1233, T(490, 'Harbor Hawks', 'HAWK', '#008B8B', '3-2'), T(489, 'Concrete Kings', 'CONC', '#808080', '1-4'), 'in_progress', { score: { home: 28, away: 35 }, series: { home: 1, away: 1 } }),
    g(1235, T(492, 'Rooftop Rebels', 'ROOF', '#E040A0', '2-3'), T(491, 'Alley Cats', 'ALLY', '#FF8C00', '4-1'), 'scheduled'),
  ];
  const table = [[482, 'Gridiron Ghosts', 'GHO', '5-0'], [493, 'Subway Surfers', 'ADAM', '4-1'], [491, 'Alley Cats', 'ALLY', '4-1'], [486, 'Football Furies', 'BLONDE', '3-2'],
    [487, 'Rail Riders', 'RAIL', '3-2'], [490, 'Harbor Hawks', 'HAWK', '3-2'], [483, 'Jock Squad', 'JOCK', '2-3'], [484, 'Night Owls', 'OWL', '2-3'], [488, 'Dune Kings', 'DUNE', '2-3'],
    [492, 'Rooftop Rebels', 'ROOF', '2-3'], [485, 'Sand Crabs', 'CRAB', '1-4'], [489, 'Concrete Kings', 'CONC', '1-4']];
  h.standings = table.map(([team_id, name, abbr, record], i) => ({ rank: i + 1, team_id, name, abbr, logo_url: null, record }));
  h.recent = [...h.recent, { fixture_id: 1160, week: 1, opponent_abbr: 'CRAB', home: true, result: 'W', score: '35-14', series: '2-0', forfeit: false }];
  h.leaders = {
    pass: [{ poid: 12, name: 'Michael Vick', value: 14 }, { poid: 13, name: 'Donovan McNabb', value: 6 }],
    rush: [{ poid: 20, name: 'Barry Sanders', value: 9 }, { poid: 12, name: 'Michael Vick', value: 5 }, { poid: 21, name: 'Jerome Bettis', value: 3 }],
    rec: [{ poid: 84, name: 'Randy Moss', value: 7 }, { poid: 80, name: 'Jerry Rice', value: 4 }],
  };
  h.roster = [[12, 'Michael Vick', 'QB', 7], [20, 'Barry Sanders', 'HB', 20], [84, 'Randy Moss', 'WR', 84], [80, 'Jerry Rice', 'WR', 80], [55, 'Junior Seau', 'LB', 55],
    [92, 'Reggie White', 'DE', 92], [24, 'Charles Woodson', 'CB', 24]].map(([poid, name, position, number]) => ({ poid, name, position, number }));
  return h;
}
function leagueStates() {
  const LEAGUE_HOME = leagueHome();
  const LEAGUE_EDITION = JSON.parse(readFileSync(path.join(root, 'Tools/fixtures/league-edition-status.json'), 'utf8'));
  const home = (patch) => ({ ...structuredClone(LEAGUE_HOME), ...patch });
  const matchup = (patch) => ({ ...structuredClone(LEAGUE_HOME.matchup), ...patch });
  const oppId = LEAGUE_HOME.matchup.opponent.discord_user_id;
  const L = (name, data = {}, extra = {}) => ({ name, url: 'league.html', scope: 'main', data: { league_member: true, league_home: LEAGUE_HOME, league_edition_status: LEAGUE_EDITION, ...data }, ...extra });
  const live = home({ matchup: matchup({ status: 'live', series: { home: 1, away: 0 }, score: { home: 21, away: 14 },
    room: { room_id: 'r1', code: 'K7QP2M', members: 2, expires_at: null, state: 'in_series', host_discord_user_id: '111' } }) });
  live.week_games = live.week_games.map((w) => (w.fixture_id === 1234 ? { ...w, status: 'live', score: { home: 21, away: 14 }, series: { home: 1, away: 0 } } : w));
  return [
    L('league'),
    L('league-join', { league_home: home({ matchup: matchup({ room: { room_id: 'r2', code: 'K7QP2M', members: 1, expires_at: null, state: 'waiting', host_discord_user_id: oppId } }) }) }),
    L('league-waiting', { league_home: home({ matchup: matchup({ room: { room_id: 'r1', code: 'K7QP2M', members: 1, expires_at: null, state: 'waiting', host_discord_user_id: '111' } }) }) }),
    L('league-live', { league_home: live }),
    L('league-final', { league_home: home({ matchup: matchup({ status: 'final', series: { home: 2, away: 1 }, winner_team_id: 493 }) }) }),
    L('league-bye', { league_home: home({ matchup: matchup({ status: 'bye', opponent: null }) }) }),
    L('league-unscheduled', { league_home: home({ schedule_published: false, matchup: null, week_games: [] }) }),
    L('league-not-installed', { league_edition_status: { ...LEAGUE_EDITION, installed: false, version: null } }),
    L('league-update', { league_edition_status: { ...LEAGUE_EDITION, latest: '2026.10.09', update_available: true, size_bytes: 268435456 } }),
    L('league-stale', { league_home: home({ stale: true }) }),
    // 3a final fix M1: a cached answer with a room in it offers no Join/Cancel and says the room status is away.
    L('league-stale-room', { league_home: home({ stale: true, matchup: matchup({ room: { room_id: 'r2', code: 'K7QP2M', members: 1, expires_at: null, state: 'waiting', host_discord_user_id: oppId } }) }) }),
    // 3a final fix M4/M6: an install already running when the page opens (Install disabled, its progress shown), and
    // the first check of the installed files.
    L('league-installing', (() => { const s = { ...LEAGUE_EDITION, installed: false, version: null, installing: true, progress: { phase: 'download', done: 644245094, total: 1610612736 } };
      return { league_edition_status: s, league_edition_installed: s }; })()),
    L('league-checking', (() => { const s = { ...LEAGUE_EDITION, installing: true, progress: { phase: 'verify', done: 0, total: 1, checking: true } };
      return { league_edition_status: s, league_edition_installed: s }; })()),
    // The league server goes away after a good load: Cancel leaves the room, the reload fails, the old page stays with
    // the refresh-failed line.
    L('league-refresh-failed', { league_home: { __first: home({ matchup: matchup({ room: { room_id: 'r1', code: 'K7QP2M', members: 1, expires_at: null, state: 'waiting', host_discord_user_id: '111' } }) }),
      __reject: { kind: 'unavailable', message: "The league couldn't be loaded. Check your connection and try again." } } }, { click: '[data-key="league-cancel"]' }),
    L('league-controller', { league_play_offline: { __reject: { kind: 'controller_setup', message: 'Your controller is not set up for this game yet. Set it up so it can press Start.' } } }, { click: '[data-key="league-offline"]' }),
    L('league-update-required', { league_room_open: { __reject: { kind: 'league_update_required', message: 'Update league files to play.', required: '2026.10.09' } } }, { click: '[data-key="league-primary"]' }),
    L('league-error', { league_home: { __reject: { kind: 'unavailable', message: "The league couldn't be loaded. Check your connection and try again." } } }),
    L('league-signed-out', { league_home: { signed_in: false } }),
    L('league-small', {}, { viewport: { width: 800, height: 560 } }),
    L('league-small-live', { league_home: live }, { viewport: { width: 800, height: 560 } }),
    L('league-pad', {}, { pad: true }),
  ];
}

/** The post-game screen's states (series-over.js): who won, the final series, each game, Elo when sent, Box score. */
function overStates(N, mm, empty, match, PICK) {
  const id = MATCH_5C8.match.id;
  // You (adamknowsball) in seat 0 = the 49ers' seat, which won 2-1.
  const youSeat0 = structuredClone(MATCH_5C8);
  youSeat0.seats = [MATCH_5C8.seats[1], MATCH_5C8.seats[0]];
  const done = (m) => match({ id, mode: 'ranked', status: 'complete', relay_state: 'complete', best_of: 3, wins_needed: 2, your_accepted: true, your_ready: true,
    opponent: 'Playz', series: [2, 1], series_live: [2, 1], outcome: { kind: 'win', winner_seat: 0, official: true, left_seat: null }, seat: 0, ...m });
  const withElo = structuredClone(youSeat0);
  withElo.seats[0].elo_delta = 18;
  withElo.seats[1].elo_delta = -18;
  // A casual best of 3 (Adam 10-04: casual closes out after its decider too), unofficial until the server confirms.
  const casual = structuredClone(youSeat0);
  casual.match.mode = 'casual';
  // Playz left after game 2 at 1-1: the two decided games are listed.
  const forfeit = structuredClone(youSeat0);
  forfeit.games = forfeit.games.slice(0, 2);
  return [
    N('online-over-win', { mm_state: mm({ ...empty, match: done({}) }), live_match: youSeat0 }, { init: PICK('ranked') }),
    N('online-over-loss', { mm_state: mm({ ...empty, match: done({ seat: 1, opponent: 'Playz' }) }), live_match: MATCH_5C8 }, { init: PICK('ranked') }),
    N('online-over-elo', { mm_state: mm({ ...empty, match: done({}) }), live_match: withElo }, { init: PICK('ranked') }),
    N('online-over-casual', { mm_state: mm({ ...empty, match: done({ mode: 'casual', outcome: { kind: 'win', winner_seat: 0, official: false, left_seat: null } }) }),
      live_match: casual }, { init: PICK('casual') }),
    N('online-over-forfeit', { mm_state: mm({ ...empty, match: done({ series: [1, 1], series_live: [1, 1], outcome: { kind: 'forfeit', winner_seat: 0, official: true, left_seat: 1 } }) }),
      live_match: forfeit }, { init: PICK('ranked') }),
    N('online-over-split', { mm_state: mm({ ...empty, match: done({ mode: 'casual', best_of: 2, series: [1, 1], series_live: [1, 1], outcome: { kind: 'split', winner_seat: null, official: true, left_seat: null } }) }),
      live_match: { ...casual, games: casual.games.slice(0, 2) } }, { init: PICK('casual') }),
    // Before the live snapshot answers (or a server with none): the screen still says who won and the series.
    N('online-over-bare', { mm_state: mm({ ...empty, match: done({}) }), live_match: null }, { init: PICK('ranked') }),
    N('online-over-small', { mm_state: mm({ ...empty, match: done({}) }), live_match: youSeat0 }, { init: PICK('ranked'), viewport: { width: 800, height: 560 } }),
    N('online-over-pad', { mm_state: mm({ ...empty, match: done({}) }), live_match: youSeat0 }, { init: PICK('ranked'), pad: true }),
    // Back to Online: the idle tiles, and the screen does not come back on the next poll.
    N('online-over-back', { mm_state: mm({ ...empty, match: done({}) }), live_match: youSeat0 }, { init: PICK('ranked'), click: '[data-key="over-back"]' }),
  ];
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
    session_active: false, ui_notices: [], netplay_status: null, netplay_pending_join: null, mm_state: null, mm_launch_prefs: null,
    settings_paths: { runtime: null, game: null, userDir: null, legacyRoot: null },
    league_member: false,
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

export async function openState(browser, base, state, { viewport = { width: 1280, height: 800 }, glass = false, slow = false, reducedMotion = 'reduce', pad = false, data = null, init = null, realImages = false } = {}) {
  const context = await browser.newContext({ viewport, reducedMotion, timezoneId: 'UTC', locale: 'en-US' });
  // `init`: an extra script run before the page's own (a check that drives the page's timers itself, say).
  if (init) await context.addInitScript(init);
  const png = readFileSync(path.join(ui, 'art/cover.png'));
  // `realImages`: the backend's catalog images and Discord avatars load for real (a screenshot to compare with the site).
  if (!realImages) await context.route(/^https:\/\/(api\.nflstreetreborn\.com|cdn\.discordapp\.com)\//, (route) => route.fulfill({ status: 200, contentType: 'image/png', body: png }));
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
      if (cmd === 'stats_game' && data.stats_game_by_id && args.gameId in data.stats_game_by_id) return data.stats_game_by_id[args.gameId];
      if (cmd === 'player_profile' && args.before) return { ok: true, matches: (data.player_profile_all || []).filter((m) => m.at < args.before).slice(0, 10) };
      if (cmd === 'launch') return new Promise(() => {});
      // `{ __reject: 'why' }`: the command fails with that sentence (a server that refuses, or lacks the route).
      // `{ __first: answer, __reject: 'why' }`: the first call answers, every later one fails (a server lost after a good load).
      if (cmd in data) { const v = data[cmd];
        // An answer that depends on one argument: { __byArg: 'teamId', values: { '482': ... } } (league_logo).
        if (v && typeof v === 'object' && '__byArg' in v) return v.values[String(args?.[v.__byArg])] ?? null;
        if (v && typeof v === 'object' && '__first' in v) { if (!v.__used) { v.__used = true; return v.__first; } throw v.__reject; }
        if (v && typeof v === 'object' && '__reject' in v) throw v.__reject; return v; }
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
