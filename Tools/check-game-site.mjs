// Every box score is ONE page (owner 10-03, "it opens the wrong style box score page"): match.html, the site's
// /live/<match> page inside the launcher, its controller (site/live-page.js) and renderers vendored unchanged into
// ui/site/. A MATCHMADE match opens it by its id on the real 5c8d58ca snapshot (Tools/fixtures/match-5c8d58ca.json, as
// live_match answers it); a game that is not (an offline game, an unlinked one, a record's game) by its games, the
// snapshot built from stats_game by game-snapshot.js -- on the site's 644488b4 fixture (Tools/fixtures/
// game-644488b4.json). This checks the strip (full team names, both players, the G1.. line score), the three tabs, the
// empty and totals-only words, that no text says undefined/NaN/null or carries an en dash, that every image comes from
// a prefix the CSP allows, that tabs, plays and drives are reachable by pad, that Back returns where the page was opened
// from, and that the vendored files match the backend. Screens at 1280x800 for each tab and for the match.
//     node Tools/check-game-site.mjs [outdir]      (screens in target/game-site by default)
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import assert from 'node:assert/strict';
import { serve, openState, root, GAME_644, GAME_ID, GAME_3, MATCH_URL, matchGames, MATCH_5C8, MATCH_PAGE_URL } from './lib/page-states.mjs';

const out = path.resolve(process.argv[2] || path.join(root, 'target/game-site'));
mkdirSync(out, { recursive: true });
// The vendored files are what a sync would write (skipped when the backend is not on disk).
execFileSync(process.execPath, [path.join(root, 'Tools/sync-site-render.mjs'), '--check'], { stdio: 'inherit' });

const ALLOWED = ['https://api.nflstreetreborn.com/live-assets/', 'https://cdn.discordapp.com/avatars/'];
const ID = GAME_644.game.id;
const STATE = (name, extra = {}) => ({ name, url: `match.html?game=${ID}`, scope: 'main', data: { stats_game: GAME_644 }, ...extra });
const srv = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
const texts = (page, sel) => page.locator(sel).evaluateAll((ns) => ns.map((n) => n.textContent.trim()));
const shots = [];
const shot = async (page, name) => { const file = path.join(out, `${name}.png`); await page.screenshot({ path: file }); shots.push(file); };

/** Every visible word of the site page is clean: no undefined / NaN / null, no en or em dash. */
async function clean(page, where) {
  const text = await page.locator('#site-game').innerText();
  assert.doesNotMatch(text, /\bundefined\b|\bNaN\b|\bnull\b/, `${where}: no undefined, NaN or null`);
  assert.doesNotMatch(text, /[–—]/, `${where}: hyphens only, never an en or em dash`);
}
/** Every image on the page is from a prefix the CSP img-src allows (or the launcher's own art). */
async function images(page, where) {
  const srcs = await page.locator('#site-game img').evaluateAll((ns) => ns.map((n) => n.src));
  assert.ok(srcs.length > 0, `${where}: the page draws images`);
  for (const s of srcs) assert.ok(ALLOWED.some((p) => s.startsWith(p)), `${where}: image outside the CSP: ${s}`);
  const broken = await page.locator('#site-game img').evaluateAll((ns) => ns.filter((n) => n.complete && n.naturalWidth === 0).map((n) => n.src));
  assert.deepEqual(broken, [], `${where}: every image loads`);
}

try {
  { // a single game (not matchmade) on the match page: the strip as a match's (full team names, both players, Final
    // over the G1 line score, no series banner for one game), Live Feed (the default tab), All plays by drive
    const { page, errors, close } = await openState(browser, srv.base, STATE('game-feed'));
    await page.locator('#strip .strip').waitFor();
    assert.deepEqual(await texts(page, '#strip .st-name'), ['Buffalo Bills', 'Philadelphia Eagles'], 'full team names, as the match page');
    assert.deepEqual(await texts(page, '#strip .st-player'), ['adamknowsball', 'Playz']);
    assert.match(await page.locator('#strip .st-status').textContent(), /^Final/);
    assert.deepEqual(await texts(page, '#strip .st-pts'), ['8', '36']);
    assert.deepEqual(await texts(page, '#strip table.ls thead th'), ['', 'G1']);
    assert.deepEqual(await page.locator('#strip table.ls tbody tr').evaluateAll((rs) => rs.map((r) => [...r.children].map((c) => c.textContent.trim()))), [['BUF', '8'], ['PHI', '36']]);
    assert.equal(await page.locator('#strip .pill').count(), 0, 'one game: no series banner');
    assert.equal(await page.locator('#strip .st-team.a').evaluate((n) => n.classList.contains('trailing')), true, 'the trailing side is dimmed');
    assert.equal(await page.locator('body.page-match').count(), 1, 'the match page itself');
    assert.equal(await page.locator('#totals-note').count(), 0, 'a verified game: no totals-only words');
    // the site's tabs, Live Feed picked, every tab focusable (pad)
    assert.deepEqual(await texts(page, '#site-game .tabs [role="tab"]'), ['Live Feed', 'Box Score', 'Team Stats']);
    assert.deepEqual(await page.locator('#site-game [role="tab"]').evaluateAll((ns) => ns.map((n) => [n.getAttribute('aria-selected'), n.tabIndex])), [['true', 0], ['false', 0], ['false', 0]]);
    assert.equal(await page.locator('#tab-feed').isVisible(), true);
    assert.equal(await page.locator('#tab-box').isVisible(), false);
    // Live Feed: the latest-play panel and the drives, newest first, all closed after the game
    assert.equal(await page.locator('#panel .pp-wrap .cap > span').first().textContent(), 'Latest play');
    assert.equal(await page.locator('#panel .pp-step').count(), 4);
    assert.deepEqual(await texts(page, '#allplays .ap-seg button'), ['All plays', 'Scoring plays']);
    assert.equal(await page.locator('#allplays .dv').count(), GAME_644.drives.length);
    assert.equal(await page.locator('#allplays .dv.open').count(), 0, 'post-game: every drive collapsed');
    assert.equal(await page.locator('#allplays .ap-qh').count(), 0, 'one game: no Game 1 heading over the drives');
    // the launcher's row: Back (B / Esc) and Open on web (the site's /games/<id>) for an online game; no game picker
    assert.equal(await page.locator('#back').getAttribute('href'), 'stats.html');
    await page.locator('[data-key="web"]').click();
    assert.deepEqual(await page.evaluate(() => window.__calls.filter((c) => c.cmd === 'open_website').map((c) => c.args.url)), [`https://online.nflstreetreborn.com/games/${ID}`]);
    assert.equal(await page.locator('#feed-pick .picker, #tab-box .picker').count(), 0, 'one game: no picker');
    assert.deepEqual(await page.evaluate(() => window.__calls.filter((c) => c.cmd === 'live_match').length), 0, 'a game never asks live_match');
    await clean(page, 'feed');
    await images(page, 'feed');
    await shot(page, 'game-feed');
    // open a drive: its chart and play cards; picking a card shows it in the panel
    await page.locator('#allplays .dv-hd').first().click();
    await page.locator('#allplays .dv.open .ap-btn').first().waitFor();
    assert.equal(await page.evaluate(() => document.activeElement?.classList.contains('dv-hd')), true, 'focus stays on the drive row');
    await page.locator('#allplays .dv.open .ap-btn').last().click();
    await page.waitForFunction(() => /^Play \d+ of \d+$/.test(document.querySelector('#panel .cap > span')?.textContent ?? ''));
    await page.locator('#panel .pp-step[data-step="first"]').click();
    assert.equal(await page.locator('#panel .cap > span').first().textContent(), `Play 1 of ${GAME_644.plays.length}`);
    await page.locator('#allplays .ap-seg button[data-mode="scoring"]').click();
    assert.equal(await page.locator('#allplays .ap-seg button[data-mode="scoring"]').getAttribute('aria-pressed'), 'true');
    await clean(page, 'feed, stepped');
    await shot(page, 'game-feed-open');
    assert.deepEqual(errors, []);
    await close();
  }
  { // Box Score: two team columns, Passing / Rushing / Receiving / Defense / Fumbles, TEAM rows, the "No ..." words.
    // The fixture without the Eagles' passer's line, so one section is empty ("No Philadelphia passing"), as on the
    // site's own page when a side never threw.
    const lines = GAME_644.lines.filter((l) => !(l.team_index === 1 && l.stats?.pass_att > 0));
    const { page, errors, close } = await openState(browser, srv.base, STATE('game-box', { click: '[data-key="tab-box"]', data: { stats_game: { ...GAME_644, lines } } }));
    assert.equal(await page.locator('#tabbtn-box').getAttribute('aria-selected'), 'true');
    assert.equal(await page.locator('#tab-box').isVisible(), true);
    assert.equal(await page.locator('#tab-feed').isVisible(), false);
    const cols = page.locator('#tab-box .bx-col');
    assert.equal(await cols.count(), 2);
    for (let i = 0; i < 2; i++) {
      const heads = await cols.nth(i).locator('.bx-sh').allInnerTexts();
      const team = i ? 'Philadelphia' : 'Buffalo';
      assert.deepEqual(heads.map((h) => h.trim()), ['Passing', 'Rushing', 'Receiving', 'Defense', 'Fumbles'].map((k) => `${team} ${k}`));
      const sections = cols.nth(i).locator('.bx-sec');
      for (let s = 0; s < 5; s++) {
        const sec = sections.nth(s);
        if (await sec.locator('table').count()) assert.equal((await sec.locator('tr.tot td.who').textContent()).trim(), 'TEAM', `${team} section ${s} ends in TEAM`);
        else assert.match(await sec.locator('.bx-none').textContent(), /^No /, `${team} section ${s}: the empty words`);
      }
    }
    assert.deepEqual(await texts(page, '#tab-box .bx-none'), ['No Philadelphia passing']);
    // the box sits in two columns side by side at 1280 wide, and tables keep their layout (theme.css's .tbl does not leak)
    const [a, b] = await cols.evaluateAll((ns) => ns.map((n) => n.getBoundingClientRect().toJSON()));
    assert.ok(Math.abs(a.top - b.top) < 1 && b.left > a.right, 'two columns side by side');
    assert.deepEqual(await page.locator('#tab-box td.who').evaluateAll((ns) => [...new Set(ns.map((n) => getComputedStyle(n).display))]), ['table-cell']);
    await clean(page, 'box');
    await images(page, 'box');
    await shot(page, 'game-box');
    assert.deepEqual(errors, []);
    await close();
  }
  { // Team Stats: the ESPN table, a logo and abbrev per side, bright and grey values
    const { page, errors, close } = await openState(browser, srv.base, STATE('game-team', { click: '[data-key="tab-team"]' }));
    assert.equal(await page.locator('#tab-team').isVisible(), true);
    assert.deepEqual(await texts(page, '#tab-team thead th'), ['Team stats', 'BUF', 'PHI']);
    const rows = await texts(page, '#tab-team tbody td.who');
    for (const r of ['Total yards', 'Passing yards', 'Rushing yards', 'Touchdowns', 'Turnovers', 'Plays']) assert.ok(rows.includes(r), `Team Stats has ${r}`);
    assert.ok(await page.locator('#tab-team td.v.hi').count() > 0 && await page.locator('#tab-team td.v.lo').count() > 0);
    await clean(page, 'team');
    await images(page, 'team');
    await shot(page, 'game-team');
    assert.deepEqual(errors, []);
    await close();
  }
  { // the small window (800x560): no sideways scroll, the box score in one column
    const { page, errors, close } = await openState(browser, srv.base, STATE('game-small', { click: '[data-key="tab-box"]' }), { viewport: { width: 800, height: 560 } });
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), 'no sideways scroll');
    assert.ok(await page.locator('#main').evaluate((n) => n.scrollWidth <= n.clientWidth), 'the page area does not scroll sideways');
    // a page this narrow is under the site's 900 px breakpoint (on the page width, sync-site-render.mjs pageQuery)
    const [a, b] = await page.locator('#tab-box .bx-col').evaluateAll((ns) => ns.map((n) => n.getBoundingClientRect().toJSON()));
    assert.ok(Math.abs(a.left - b.left) < 1 && b.top > a.bottom - 1, 'the two teams stack in one column');
    await shot(page, 'game-small-box');
    await page.locator('[data-key="tab-feed"]').click();
    await page.locator('#main').evaluate((n) => { n.scrollTop = 0; });
    await shot(page, 'game-small-feed');
    assert.deepEqual(errors, []);
    await close();
  }
  { // a controller: Down from Back reaches the tabs; A on Box Score shows it; B / Esc returns to Stats
    const { page, errors, close } = await openState(browser, srv.base, STATE('game-pad'), { pad: true });
    const press = async (i) => {
      await page.evaluate((i) => { window.pads[0].buttons[i] = { pressed: true, value: 1 }; }, i);
      await page.waitForTimeout(120);
      await page.evaluate((i) => { window.pads[0].buttons[i] = { pressed: false, value: 0 }; }, i);
      await page.waitForTimeout(120);
    };
    const key = () => page.evaluate(() => document.activeElement?.getAttribute('data-key') ?? document.activeElement?.className ?? '');
    await press(13); // D-pad down: into the page
    const seen = [];
    for (let i = 0; i < 6 && !(await key()).startsWith('tab-'); i++) { seen.push(await key()); await press(13); }
    assert.match(await key(), /^tab-/, `Down reaches the tabs (went ${seen.join(' > ')})`);
    for (let i = 0; i < 3 && (await key()) !== 'tab-box'; i++) await press(15); // D-pad right along the tabs
    assert.equal(await key(), 'tab-box', 'Right walks the tabs');
    await press(0); // A
    assert.equal(await page.locator('#tab-box').isVisible(), true, 'A on Box Score shows it');
    await page.locator('#tabbtn-feed').click();
    // the drives are buttons the pad reaches
    assert.ok(await page.locator('#allplays .dv-hd').evaluateAll((ns) => ns.every((n) => n.tabIndex >= 0)));
    await page.keyboard.press('Escape');
    await page.waitForURL(/stats\.html$/);
    assert.deepEqual(errors, []);
    await close();
  }
  { // a match that is not matchmade, every game on one page as a matchmade one: the G1..G3 line score and the series
    // banner, Game 1 · Game 2 · Game 3 in the Live Feed (landing on the last), Series total in Box Score and Team Stats;
    // the totals-only game says so above the page; each game read once
    const { page, errors, close } = await openState(browser, srv.base, { name: 'game-match', url: MATCH_URL, scope: 'main', data: { stats_game_by_id: matchGames() } });
    await page.locator('#strip .strip').waitFor();
    assert.deepEqual(await texts(page, '#strip table.ls thead th'), ['', 'G1', 'G2', 'G3']);
    assert.deepEqual(await page.locator('#strip table.ls tbody tr').evaluateAll((rs) => rs.map((r) => [...r.children].map((c) => c.textContent.trim()))),
      [['BUF', '0', '8', '8'], ['PHI', '0', '36', '36']]);
    assert.equal(await page.locator('#strip .pill').textContent(), 'PHI wins 2-0', 'the series banner (the totals-only game decided nothing)');
    assert.deepEqual(await texts(page, '#feed-pick .picker button'), ['Game 1', 'Game 2', 'Game 3']);
    assert.equal(await page.locator('#feed-pick .picker button[aria-pressed="true"]').textContent(), 'Game 3');
    assert.ok((await page.locator('#allplays').innerText()).includes('GAME 3'));
    assert.match(await page.locator('#totals-note').textContent(), /^Totals only for Game 1\. It was played before play-by-play existed/);
    assert.equal(await page.locator('#back').getAttribute('href'), 'stats.html');
    await clean(page, 'match');
    await shot(page, 'game-match');
    // Game 1, the totals-only game: the site's words for a game with no plays
    await page.locator('#feed-pick .picker button[data-pick="1"]').click();
    await page.waitForFunction(() => document.querySelector('#feed-pick .picker button[aria-pressed="true"]')?.textContent === 'Game 1');
    assert.match(await page.locator('#panel').innerText(), /No live plays reached us for this game\./);
    await clean(page, 'match, game 1');
    await shot(page, 'game-match-g1');
    await page.locator('[data-key="tab-box"]').click();
    assert.deepEqual(await texts(page, '#tab-box .picker button'), ['Game 1', 'Game 2', 'Game 3', 'Series total']);
    assert.equal(await page.locator('#tab-box .picker button[aria-pressed="true"]').textContent(), 'Series total');
    assert.equal(await page.locator('#tab-box .bx-col').count(), 2);
    await clean(page, 'match box');
    // each game is read once
    const asked = await page.evaluate(() => window.__calls.filter((c) => c.cmd === 'stats_game').map((c) => c.args.gameId));
    assert.deepEqual([...asked].sort(), [GAME_ID, GAME_644.game.id, GAME_3].sort(), `each game read once: ${asked}`);
    // the opening and closing transitions ride the cross-document view transition, on --d2. Read from the source: the
    // test browser may predate :active-view-transition-type (Chromium 125) and drop the closing rule, as it would
    // drop the whole cross-document transition (126); the launcher's WebView2 is evergreen.
    const css = await page.evaluate(async () => (await (await fetch('theme.css')).text()) + (await (await fetch('pages/game.css')).text()));
    assert.match(css, /html:has\(body\.page-game\)::view-transition-new\(page\)\{animation:game-in var\(--d2\) var\(--ease\) both\}/, 'opening: game-in on --d2');
    assert.match(css, /html:active-view-transition-type\(from-game\)::view-transition-old\(page\)\{animation:game-out var\(--d2\) var\(--ease\) both\}/, 'closing: game-out on --d2');
    assert.match(css, /@keyframes game-in\{from\{opacity:0;transform:translateY\(\d+px\)\}\}/, 'a fade with a slight rise');
    assert.deepEqual(errors, []);
    await close();
  }
  { // the match page (a matchmade match): the site's /live/<match> page on the real 5c8d58ca snapshot -- full team names,
    // both players, the G1 / G2 / G3 line score, the series banner, Game 1 · Game 2 · Game 3 landing on the last game,
    // the latest play's portrait, All plays by drive; Box Score and Team Stats for each game and the series total
    const { page, errors, close } = await openState(browser, srv.base, { name: 'match-feed', url: MATCH_PAGE_URL, scope: 'main', data: { live_match: MATCH_5C8 } });
    await page.locator('#strip .strip').waitFor();
    assert.deepEqual(await texts(page, '#strip .st-name'), ['San Francisco 49ers', 'Los Angeles Rams'], 'full team names');
    assert.deepEqual(await texts(page, '#strip .st-player'), ['Playz', 'adamknowsball']);
    assert.deepEqual(await texts(page, '#strip .st-pts'), ['36', '12']);
    assert.match(await page.locator('#strip').innerText(), /Final\s*·\s*Unofficial/);
    // the line score: a column per game, a row per team
    assert.deepEqual(await texts(page, '#strip table.ls thead th'), ['', 'G1', 'G2', 'G3']);
    assert.deepEqual(await page.locator('#strip table.ls tbody tr').evaluateAll((rs) => rs.map((r) => [...r.children].map((c) => c.textContent.trim()))),
      [['SF', '2', '36', '36'], ['LAR', '36', '0', '12']]);
    assert.equal(await page.locator('#strip .pill').textContent(), 'SF wins the series 2-1', 'the series banner');
    // the tabs, every one focusable (pad), and the Live Feed's games, the last one picked
    assert.deepEqual(await texts(page, '#site-game .tabs [role="tab"]'), ['Live Feed', 'Box Score', 'Team Stats']);
    assert.deepEqual(await page.locator('#site-game [role="tab"]').evaluateAll((ns) => ns.map((n) => n.tabIndex)), [0, 0, 0]);
    assert.deepEqual(await texts(page, '#feed-pick .picker button'), ['Game 1', 'Game 2', 'Game 3']);
    assert.equal(await page.locator('#feed-pick .picker button[aria-pressed="true"]').textContent(), 'Game 3');
    // the latest play: the touchdown catch, the receiver's portrait
    assert.match(await page.locator('#panel').innerText(), /31-yard catch[\s\S]*Mike Evans/);
    assert.ok(await page.locator('#panel img.portrait').count() > 0, 'the panel shows a portrait');
    const portraits = await page.locator('#site-game img.portrait').evaluateAll((ns) => ns.map((n) => n.src));
    assert.ok(portraits.every((s) => s.startsWith('https://api.nflstreetreborn.com/live-assets/r/') && s.includes('/portraits/')), `portraits from the catalog: ${portraits}`);
    // the site's own src is what the page reads back (a failed image is remembered under the renderers' key)
    assert.match(await page.locator('#panel img.portrait').first().evaluate((n) => n.getAttribute('src')), /^\/live-assets\/r\//);
    assert.deepEqual(await texts(page, '#allplays .ap-seg button'), ['All plays', 'Scoring plays']);
    assert.equal(await page.locator('#allplays .dv').count(), MATCH_5C8.games[2].drives.length);
    assert.ok((await page.locator('#allplays').innerText()).includes('GAME 3'));
    // the launcher's row: Back, and Open on web to the site's own /live/<match>
    assert.equal(await page.locator('#back').getAttribute('href'), 'stats.html');
    await page.locator('[data-key="web"]').click();
    assert.deepEqual(await page.evaluate(() => window.__calls.filter((c) => c.cmd === 'open_website').map((c) => c.args.url)),
      [`https://online.nflstreetreborn.com/live/${MATCH_5C8.match.id}`]);
    // the snapshot came from the live_match command, never a fetch from the page
    assert.deepEqual(await page.evaluate(() => [...new Set(window.__calls.filter((c) => c.cmd === 'live_match').map((c) => c.args.matchId))]), [MATCH_5C8.match.id]);
    await clean(page, 'match feed');
    await images(page, 'match feed');
    await shot(page, 'match-feed');
    // Game 1 in the Live Feed: its own panel and drives
    await page.locator('#feed-pick .picker button[data-pick="1"]').click();
    await page.waitForFunction(() => document.querySelector('#feed-pick .picker button[aria-pressed="true"]')?.textContent === 'Game 1');
    assert.equal(await page.locator('#allplays .dv').count(), MATCH_5C8.games[0].drives.length);
    assert.ok((await page.locator('#allplays').innerText()).includes('GAME 1'));
    await clean(page, 'match feed, game 1');
    // Box Score: Series total first (the site's default once there are two games), then each game
    await page.locator('[data-key="tab-box"]').click();
    assert.equal(await page.locator('#tab-box').isVisible(), true);
    assert.deepEqual(await texts(page, '#tab-box .picker button'), ['Game 1', 'Game 2', 'Game 3', 'Series total']);
    assert.equal(await page.locator('#tab-box .picker button[aria-pressed="true"]').textContent(), 'Series total');
    for (const pick of ['total', '1', '2', '3']) {
      await page.locator(`#tab-box .picker button[data-pick="${pick}"]`).click();
      await page.waitForFunction((p) => document.querySelector('#tab-box .picker button[aria-pressed="true"]')?.dataset.pick === p, pick);
      const cols = page.locator('#tab-box .bx-col');
      assert.equal(await cols.count(), 2, `box ${pick}: two teams`);
      assert.equal((await cols.nth(0).locator('.bx-sh').first().innerText()).trim(), 'San Francisco Passing', `box ${pick}: the full team name`);
      await clean(page, `match box ${pick}`);
      await images(page, `match box ${pick}`);
    }
    await page.locator('#tab-box .picker button[data-pick="total"]').click();
    await shot(page, 'match-box');
    // Team Stats: the same picker, each game
    await page.locator('[data-key="tab-team"]').click();
    for (const pick of ['total', '1', '2', '3']) {
      await page.locator(`#tab-team .picker button[data-pick="${pick}"]`).click();
      await page.waitForFunction((p) => document.querySelector('#tab-team .picker button[aria-pressed="true"]')?.dataset.pick === p, pick);
      assert.deepEqual(await texts(page, '#tab-team thead th'), ['Team stats', 'SF', 'LAR'], `team ${pick}`);
      assert.ok((await texts(page, '#tab-team tbody td.who')).includes('Total yards'));
      await clean(page, `match team ${pick}`);
    }
    await page.locator('#tab-team .picker button[data-pick="total"]').click();
    await shot(page, 'match-team');
    assert.deepEqual(errors, []);
    await close();
  }
  { // the match page by controller: Down reaches the tabs, Right walks them, A shows Box Score; B / Esc goes Back
    const { page, errors, close } = await openState(browser, srv.base, { name: 'match-pad', url: MATCH_PAGE_URL, scope: 'main', data: { live_match: MATCH_5C8 } }, { pad: true });
    const press = async (i) => {
      await page.evaluate((i) => { window.pads[0].buttons[i] = { pressed: true, value: 1 }; }, i);
      await page.waitForTimeout(120);
      await page.evaluate((i) => { window.pads[0].buttons[i] = { pressed: false, value: 0 }; }, i);
      await page.waitForTimeout(120);
    };
    const key = () => page.evaluate(() => document.activeElement?.getAttribute('data-key') ?? document.activeElement?.className ?? '');
    const seen = [];
    for (let i = 0; i < 8 && !(await key()).startsWith('tab-'); i++) { await press(13); seen.push(await key()); }
    assert.match(await key(), /^tab-/, `Down reaches the tabs (went ${seen.join(' > ')})`);
    for (let i = 0; i < 3 && (await key()) !== 'tab-box'; i++) await press(15);
    assert.equal(await key(), 'tab-box', 'Right walks the tabs');
    await press(0);
    assert.equal(await page.locator('#tab-box').isVisible(), true, 'A on Box Score shows it');
    assert.deepEqual(await page.locator('#site-game [role="tab"]').evaluateAll((ns) => ns.map((n) => n.tabIndex)), [0, 0, 0], 'every tab stays reachable');
    assert.ok(await page.locator('#tab-box .picker button').evaluateAll((ns) => ns.length > 0 && ns.every((n) => n.tabIndex >= 0)), 'the games are reachable');
    await page.locator('#tabbtn-feed').click();
    assert.ok(await page.locator('#allplays .dv-hd').evaluateAll((ns) => ns.length > 0 && ns.every((n) => n.tabIndex >= 0)), 'the drives are reachable');
    await page.keyboard.press('Escape');
    await page.waitForURL(/stats\.html$/);
    assert.deepEqual(errors, []);
    await close();
  }
  { // a match the server does not have (live_match answers null, the backend's 404): the site's words; a bad id too
    const { page, errors, close } = await openState(browser, srv.base, { name: 'match-gone', url: MATCH_PAGE_URL, scope: 'main', data: { live_match: null } });
    await page.waitForFunction(() => document.querySelector('#strip .empty')?.textContent === 'No such match.');
    await page.goto(`${srv.base}/match.html?match=nope`);
    await page.locator('#strip .empty').waitFor();
    assert.equal(await page.locator('#strip .empty').textContent(), 'No such match.');
    assert.equal(await page.locator('#site-game .tabs').isVisible(), false);
    assert.equal(await page.locator('#back').isVisible(), true);
    assert.deepEqual(errors, []);
    await close();
  }
  { // Back returns where the match was opened from (a profile's Matches tab, say), validated
    const from = 'profile.html?player=0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d#matches';
    const { page, errors, close } = await openState(browser, srv.base, { name: 'game-from', url: `match.html?game=${GAME_644.game.id}&from=${encodeURIComponent(from)}`, scope: 'main', data: { stats_game: GAME_644 } });
    await page.locator('#strip .strip').waitFor();
    assert.equal(await page.locator('#back').getAttribute('href'), from);
    await page.goto(`${srv.base}/match.html?game=${GAME_644.game.id}&from=${encodeURIComponent('https://evil.example/')}`);
    await page.locator('#back').waitFor();
    assert.equal(await page.locator('#back').getAttribute('href'), 'stats.html');
    assert.deepEqual(errors, []);
    await close();
  }
  { // an unverified game: the strip, the site's totals-only words above the page, the site's empty words in the tabs
    const { page, errors, close } = await openState(browser, srv.base, { name: 'game', url: `match.html?game=${GAME_ID}`, scope: 'main' });
    await page.locator('#strip .strip').waitFor();
    assert.match(await page.locator('#totals-note').textContent(), /^Totals only for this game\./);
    assert.match(await page.locator('#panel').innerText(), /No live plays reached us for this game\./);
    await page.locator('[data-key="tab-box"]').click();
    assert.equal(await page.locator('#tab-box .empty').textContent(), 'No box score for this game yet.');
    await clean(page, 'unverified');
    await shot(page, 'game-unverified');
    assert.deepEqual(errors, []);
    await close();
  }
  { // a bad id, or a game the server does not have: the site's words, no tabs, Back still there
    const { page, errors, close } = await openState(browser, srv.base, { name: 'game-missing', url: 'match.html?game=nope', scope: 'main', data: { stats_game: null } });
    assert.equal(await page.locator('#strip .empty').textContent(), "That game couldn't be found.");
    assert.equal(await page.locator('#site-game .tabs').isVisible(), false);
    assert.equal(await page.locator('#back').isVisible(), true);
    await page.goto(`${srv.base}/match.html?game=${GAME_3}`);
    await page.waitForFunction(() => document.querySelector('#strip .empty')?.textContent === "That game couldn't be found.");
    assert.equal(await page.locator('#site-game .tabs').isVisible(), false);
    assert.deepEqual(errors, []);
    await close();
  }
  console.log(`check-game-site: PASS (${shots.length} screens in ${out})`);
} finally {
  await browser.close();
  await srv.close();
}
