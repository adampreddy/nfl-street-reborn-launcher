// The Stats and Profile components in theme.css (spec §3.3, §3.4, §5.2): the 40px section header with or without tabs,
// the match list's row heights and result colours, the shared table grid and Total row, the portrait on the team
// colour, the cover wash from the avatar's colours, the best-of-3 reveal and its motion, the live dot.
//     node Tools/check-stat-components.mjs
import { chromium } from 'playwright';
import assert from 'node:assert/strict';
import { serve, settle } from './lib/page-states.mjs';

const html = `<!doctype html><html lang="en" data-glass="off" data-motion="slow"><head><meta charset="utf-8"><link rel="stylesheet" href="theme.css"></head>
<body class="shell themed page-test"><main id="main"><section id="screen" style="width:900px;padding:24px">
<div class="sec-head" id="h1"><h2>Your matches</h2><a class="sec-link" href="#">Your profile ›</a></div>
<div class="sec-head" id="h2"><h2>Match history</h2><div class="tabs"><span class="tab-ind"></span><button class="tab" aria-pressed="true">All</button><button class="tab" aria-pressed="false">Ranked</button></div></div>
<div class="glass m-list" id="list"><div class="m-row m-hd"><span>Result</span><span>Matchup</span><span>Opponent</span><span>Elo</span><span>Date</span><span></span></div>
<button class="m-row" id="row" aria-expanded="false"><span class="m-res"><b class="W">W</b><span>2–1</span></span><span class="m-mu"></span><span class="m-opp"></span><span class="m-elo up">+16</span><span class="m-date">Sep 26</span><span class="chev"></span></button>
<div class="rows-reveal" id="rev"><div inert><button class="m-row m-game" id="g1"><span class="m-res"><b class="L">L</b><span>20–31</span></span><span></span><span></span><span></span><span></span><span></span></button></div></div></div>
<table class="tbl" id="t1"><thead><tr><th>Player</th><th>YDS</th><th>TD</th></tr></thead><tbody><tr><th scope="row">A</th><td>1</td><td>2</td></tr><tr class="total"><th scope="row">Total</th><td>1</td><td>2</td></tr></tbody></table>
<table class="tbl" id="t2"><thead><tr><th>Player</th><th>CAR</th><th>YDS</th><th>AVG</th></tr></thead><tbody><tr><th scope="row">B</th><td>1</td><td>2</td><td>3</td></tr></tbody></table>
<span class="player-face" id="pf" style="--tc:#A71930"><span class="face letter">A</span></span><span class="player-face" id="pf0"><span class="face letter">B</span></span>
<div class="glass" id="card" style="position:relative;height:80px;--c1:#A71930;--c2:#2b0a10"><div class="cover-wash"></div></div>
<span class="live-dot" id="live"></span>
</section></main></body></html>`;

const srv = await serve();
const browser = await chromium.launch({ channel: process.env.REBORN_TEST_BROWSER || 'msedge', headless: true });
try {
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  await context.route('**/fixture.html', (r) => r.fulfill({ contentType: 'text/html', body: html }));
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  await page.goto(`${srv.base}/fixture.html`);
  await page.evaluate(() => document.fonts.ready);
  await settle(page); // P4-8: #main runs look-enter; measure only once every finite animation is done
  const h = (sel) => page.locator(sel).evaluate((n) => n.getBoundingClientRect().height);
  const css = (sel, prop) => page.locator(sel).evaluate((n, p) => getComputedStyle(n)[p], prop);
  // 40px section headers, tabs or not (spec §3.3)
  assert.equal(await h('#h1'), 40);
  assert.equal(await h('#h2'), 40);
  assert.equal(await h('#h2 .tabs'), 40, 'tabs fit the 40px header');
  // row heights (spec §3.3): header 36, match 56, game 40 once open; W and L colours
  assert.equal(await h('#list .m-hd'), 36);
  assert.equal(await h('#row'), 56);
  assert.equal(await css('#row .m-res b', 'color'), 'rgb(61, 220, 151)', 'W is --ok');
  assert.equal(await css('#g1 .m-res b', 'color'), 'rgb(255, 122, 127)', 'L is --red-ink');
  // the reveal: closed = 0 high, at --d3; open = the games, and the chevron turns
  assert.equal(await h('#rev'), 0);
  assert.equal(await css('#rev', 'transitionDuration'), '0.96s', 'slow: --d3');
  const shut = await css('#row .chev', 'transform');
  await page.evaluate(() => {
    document.getElementById('rev').classList.add('open');
    document.querySelector('#rev>div').removeAttribute('inert');
    document.getElementById('row').setAttribute('aria-expanded', 'true');
  });
  await page.waitForFunction(() => document.getElementById('rev').getBoundingClientRect().height === 40);
  assert.equal(await h('#g1'), 40);
  assert.notEqual(await css('#row .chev', 'transform'), shut, 'the chevron turns');
  await page.evaluate(() => { document.documentElement.dataset.motion = 'quick'; });
  assert.equal(await css('#rev', 'transitionDuration'), '0.24s', 'quick: --d3');
  // tables share one grid: a 220px name column, then the numbers start on the same edge
  const cols = await page.evaluate(() => ['#t1', '#t2'].map((t) => {
    const first = document.querySelector(`${t} th`).getBoundingClientRect(), second = document.querySelector(`${t} th:nth-child(2)`).getBoundingClientRect();
    return [Math.round(first.width), Math.round(second.left)];
  }));
  assert.deepEqual(cols[0], cols[1]);
  assert.equal(cols[0][0], 220);
  assert.equal(await css('#t1 tr.total th', 'borderTopColor'), 'rgba(255, 255, 255, 0.16)', 'Total row: --stroke-hi');
  assert.equal(await css('#t1 thead th:nth-child(2)', 'textTransform'), 'uppercase');
  // the portrait follows its team colour; the cover wash uses the avatar's colours
  assert.notEqual(await css('#pf .face', 'backgroundImage'), await css('#pf0 .face', 'backgroundImage'), 'the face follows --tc');
  assert.match(await css('#card .cover-wash', 'backgroundImage'), /rgb\(167, 25, 48\)/);
  assert.equal(await css('#live', 'animationName'), 'live-pulse');
  // reduced motion: instant
  await page.emulateMedia({ reducedMotion: 'reduce' });
  assert.match(await css('#rev', 'transitionDuration'), /^0s/);
  assert.equal(await css('#live', 'animationName'), 'none');
  assert.deepEqual(errors, []);
  console.log('check-stat-components: PASS');
} finally { await browser.close(); await srv.close(); }
