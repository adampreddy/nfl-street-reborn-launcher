import { chromium } from 'playwright';
const b = await chromium.launch({ channel: 'msedge', headless: true });
const p = await (await b.newContext({ viewport: { width: 1440, height: 1000 } })).newPage();
await p.goto(process.argv[2]); await p.waitForTimeout(1000);
const n = await p.locator('.win').count();
for (let i = 0; i < n; i++) await p.locator('.win').nth(i).screenshot({ path: process.argv[3] + i + '.png' });
await b.close(); console.log(n);
