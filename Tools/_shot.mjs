import { chromium } from 'playwright';
const b = await chromium.launch({ channel: 'msedge', headless: true });
const p = await (await b.newContext({ viewport: { width: 1440, height: 1000 } })).newPage();
await p.goto(process.argv[2]); await p.waitForTimeout(800);
const win = p.locator('.win'); await win.screenshot({ path: process.argv[3] });
await b.close();
