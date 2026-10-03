// The game page (Stats, then a game) is the site's /games/<id> page inside the launcher: the scoreboard strip, then
// Live Feed | Box Score | Team Stats, drawn by the site's own renderers vendored into ui/site/. On the site's 644488b4
// fixture (Tools/fixtures/game-644488b4.json, as stats_game answers it) this checks the strip, the three tabs, the
// empty words, that no text says undefined/NaN/null or carries an en dash, that every image comes from a prefix the
// CSP allows, that tabs, plays and drives are reachable by pad, and that the vendored files match the backend. A match
// of several games is ONE page (owner 10-03): the site's game picker and the series above the scoreboard swap the game
// in place, with no Previous / Next; Back returns where the match was opened from. Screens at 1280x800 for each tab and
// for the match.
//     node Tools/check-game-site.mjs [outdir]      (screens in target/game-site by default)
import { chromium } from 'playwright';
import { mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import assert from 'node:assert/strict';
import { serve, openState, root, GAME_644, GAME_ID, GAME_3, MATCH_URL, matchGames } from './lib/page-states.mjs';

const out = path.resolve(process.argv[2] || path.join(root, 'target/game-site'));
mkdirSync(out, { recursive: true });
// The vendored files are what a sync would write (skipped when the backend is not on disk).
execFileSync(process.execPath, [path.join(root, 'Tools/sync-site-render.mjs'), '--check'], { stdio: 'inherit' });

const ALLOWED = ['https://api.nflstreetreborn.com/live-assets/', 'https://cdn.discordapp.com/avatars/'];
const ID = GAME_644.game.id;
const STATE = (name, extra = {}) => ({ name, url: `game.html?game=${ID}`, scope: 'main', data: { stats_game: GAME_644 }, ...extra });
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
  { // Live Feed (the default tab): strip, latest-play panel, All plays by drive
    const { page, errors, close } = await openState(browser, srv.base, STATE('game-feed'));
    await page.locator('#strip .strip').waitFor();
    // the strip: both teams, both players, both scores, Final over the one-column line score
    const strip = await page.locator('#strip').innerText();
    for (const w of ['Bills', 'Eagles', 'adamknowsball', 'Playz', 'Final']) assert.ok(strip.includes(w), `strip names ${w}: ${strip}`);
    assert.deepEqual(await texts(page, '#strip .st-pts'), ['8', '36']);
    assert.equal(await page.locator('#strip .st-team.a').evaluate((n) => n.classList.contains('trailing')), true, 'the trailing side is dimmed');
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
    assert.equal(await page.locator('#note').textContent(), 'Final');
    // the launcher's row: Back (B / Esc) and Open on web for an online game; no Previous / Next, no picker for one game
    assert.equal(await page.locator('#back').getAttribute('href'), 'stats.html');
    assert.equal(await page.locator('[data-key="web"]').isVisible(), true);
    assert.equal(await page.locator('[data-key="game-prev"], [data-key="game-next"]').count(), 0, 'no Previous / Next');
    assert.equal(await page.locator('#game-pick').isVisible(), false, 'one game: no picker');
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
    // The fixture without the Eagles' passer's line, so one section is empty ("No Eagles passing"), as on the site's
    // own game page when a side never threw.
    const lines = GAME_644.lines.filter((l) => !(l.team_index === 1 && l.stats?.pass_att > 0));
    const { page, errors, close } = await openState(browser, srv.base, STATE('game-box', { click: '[data-key="tab-box"]', data: { stats_game: { ...GAME_644, lines } } }));
    assert.equal(await page.locator('#tabbtn-box').getAttribute('aria-selected'), 'true');
    assert.equal(await page.locator('#tab-box').isVisible(), true);
    assert.equal(await page.locator('#tab-feed').isVisible(), false);
    const cols = page.locator('#tab-box .bx-col');
    assert.equal(await cols.count(), 2);
    for (let i = 0; i < 2; i++) {
      const heads = await cols.nth(i).locator('.bx-sh').allInnerTexts();
      const team = i ? 'Eagles' : 'Bills';
      assert.deepEqual(heads.map((h) => h.trim()), ['Passing', 'Rushing', 'Receiving', 'Defense', 'Fumbles'].map((k) => `${team} ${k}`));
      const sections = cols.nth(i).locator('.bx-sec');
      for (let s = 0; s < 5; s++) {
        const sec = sections.nth(s);
        if (await sec.locator('table').count()) assert.equal((await sec.locator('tr.tot td.who').textContent()).trim(), 'TEAM', `${team} section ${s} ends in TEAM`);
        else assert.match(await sec.locator('.bx-none').textContent(), /^No /, `${team} section ${s}: the empty words`);
      }
    }
    assert.deepEqual(await texts(page, '#tab-box .bx-none'), ['No Eagles passing']);
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
  { // a whole match on one page: Game 1 · Game 2 · Game 3 and the series over the scoreboard, the last game shown first;
    // picking a game swaps the page below in place (the tab stays), the address follows without a history entry
    const { page, errors, close } = await openState(browser, srv.base, { name: 'game-match', url: MATCH_URL, scope: 'main', data: { stats_game_by_id: matchGames() } }, { reducedMotion: 'no-preference' });
    await page.locator('#strip .strip').waitFor();
    assert.deepEqual(await texts(page, '#game-pick .picker button'), ['Game 1', 'Game 2', 'Game 3']);
    assert.equal(await page.locator('#game-pick .picker button[aria-pressed="true"]').textContent(), 'Game 3');
    assert.equal(await page.locator('#game-pick .game-series').textContent(), 'Series 2-1');
    { const pk = await page.locator('#game-pick').boundingBox(), st = await page.locator('#strip').boundingBox();
      assert.ok(pk.y + pk.height <= st.y + 1, 'the picker sits above the scoreboard'); }
    assert.equal(await page.locator('[data-key="game-prev"], [data-key="game-next"]').count(), 0, 'no Previous / Next');
    assert.equal(await page.locator('#back').getAttribute('href'), 'stats.html');
    await clean(page, 'match');
    await shot(page, 'game-match');
    const histLen = await page.evaluate(() => history.length);
    await page.locator('[data-key="tab-box"]').click();
    await page.locator('[data-key="game-2"]').click();
    assert.equal(await page.evaluate(() => document.body.querySelector('#game-body').getAnimations().map((a) => a.animationName).join()), 'game-in', 'the swap rises in');
    { const [dur, d2] = await page.evaluate(() => [getComputedStyle(document.getElementById('game-body')).animationDuration, getComputedStyle(document.documentElement).getPropertyValue('--d2').trim()]);
      assert.equal(parseFloat(dur) * 1000, parseFloat(d2), `the swap runs on --d2: ${dur} vs ${d2}`); }
    await page.waitForFunction(() => document.querySelector('#game-pick button[aria-pressed="true"]')?.textContent === 'Game 2');
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'game-2', 'focus stays on the picker');
    assert.equal(new URL(page.url()).searchParams.get('game'), GAME_644.game.id, 'the address follows the game');
    assert.equal(await page.evaluate(() => history.length), histLen, 'no history entry: Back still leaves the match');
    assert.equal(await page.locator('#tabbtn-box').getAttribute('aria-selected'), 'true', 'the tab stays picked');
    assert.equal(await page.locator('#tab-box .bx-col').count(), 2);
    // the left / right arrows walk the picker, never other games or pages
    await page.keyboard.press('ArrowLeft');
    assert.equal(await page.evaluate(() => document.activeElement?.getAttribute('data-key')), 'game-1');
    assert.match(page.url(), /game.html/);
    await page.keyboard.press('Enter');
    await page.waitForFunction(() => /^Totals only/.test(document.querySelector('#tab-box .empty')?.textContent ?? ''));
    assert.equal(new URL(page.url()).searchParams.get('game'), GAME_ID);
    await clean(page, 'match, game 1');
    await shot(page, 'game-match-g1');
    // each game is read once: going back to one already seen asks nothing
    const asked = await page.evaluate(() => window.__calls.filter((c) => c.cmd === 'stats_game').map((c) => c.args.gameId));
    assert.deepEqual([...asked].sort(), [GAME_ID, GAME_644.game.id, GAME_3].sort(), `each game read once: ${asked}`);
    await page.locator('[data-key="game-3"]').click();
    await page.waitForFunction(() => document.querySelector('#game-pick button[aria-pressed="true"]')?.textContent === 'Game 3');
    assert.equal(await page.evaluate(() => window.__calls.filter((c) => c.cmd === 'stats_game').length), asked.length);
    // the opening and closing transitions ride the cross-document view transition, on --d2. Read from the source: the
    // test browser may predate :active-view-transition-type (Chromium 125) and drop the closing rule, as it would
    // drop the whole cross-document transition (126); the launcher's WebView2 is evergreen.
    const css = await page.evaluate(async () => (await (await fetch('theme.css')).text()) + (await (await fetch('pages/game.css')).text()));
    assert.match(css, /html:has\(body\.page-game\)::view-transition-new\(page\)\{animation:game-in var\(--d2\) var\(--ease\) both\}/, 'opening: game-in on --d2');
    assert.match(css, /html:active-view-transition-type\(from-game\)::view-transition-old\(page\)\{animation:game-out var\(--d2\) var\(--ease\) both\}/, 'closing: game-out on --d2');
    assert.match(css, /@keyframes game-in\{from\{opacity:0;transform:translateY\(\d+px\)\}\}/, 'a fade with a slight rise');
    assert.match(css, /prefers-reduced-motion:reduce\)\{\.game-body\.game-swap\{animation:none\}\}/, 'reduced motion: no swap animation');
    assert.deepEqual(errors, []);
    await close();
  }
  { // Back returns where the match was opened from (a profile's Matches tab, say), validated
    const from = 'profile.html?player=0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d#matches';
    const { page, errors, close } = await openState(browser, srv.base, { name: 'game-from', url: `game.html?game=${GAME_644.game.id}&from=${encodeURIComponent(from)}`, scope: 'main', data: { stats_game: GAME_644 } });
    await page.locator('#strip .strip').waitFor();
    assert.equal(await page.locator('#back').getAttribute('href'), from);
    await page.goto(`${srv.base}/game.html?game=${GAME_644.game.id}&from=${encodeURIComponent('https://evil.example/')}`);
    await page.locator('#back').waitFor();
    assert.equal(await page.locator('#back').getAttribute('href'), 'stats.html');
    assert.deepEqual(errors, []);
    await close();
  }
  { // an unverified game: the strip, then the site's totals-only words under every tab
    const { page, errors, close } = await openState(browser, srv.base, { name: 'game', url: `game.html?game=${GAME_ID}`, scope: 'main' });
    await page.locator('#strip .strip').waitFor();
    assert.match(await page.locator('#tab-feed .empty').textContent(), /^Totals only for this game\./);
    await page.locator('[data-key="tab-box"]').click();
    assert.match(await page.locator('#tab-box .empty').textContent(), /^Totals only for this game\./);
    await clean(page, 'unverified');
    await shot(page, 'game-unverified');
    assert.deepEqual(errors, []);
    await close();
  }
  { // a bad id: the site's words, no tabs, Back still there
    const { page, errors, close } = await openState(browser, srv.base, { name: 'game-missing', url: 'game.html?game=nope', scope: 'main' });
    assert.equal(await page.locator('#strip .empty').textContent(), "That game couldn't be found.");
    assert.equal(await page.locator('#site-game .tabs').isVisible(), false);
    assert.equal(await page.locator('#back').isVisible(), true);
    assert.deepEqual(errors, []);
    await close();
  }
  console.log(`check-game-site: PASS (${shots.length} screens in ${out})`);
} finally {
  await browser.close();
  await srv.close();
}
