// Screenshots of the page UI on phone-sized screens inside an emulated
// Telegram fullscreen Mini App (safe areas and the header buttons' band).
//   node scripts/ui-shots.mjs [outDir] [lobby,fight,result]
// Shots: lobby, a fight a few seconds in and the end of a training match,
// in portrait and landscape. Look at the pictures: tests do not see layout.
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createArena } from '../server/index.mjs';
process.env.PLAYWRIGHT_BROWSERS_PATH ||= resolve('.browsers');
const { chromium } = await import('playwright');

const [out = 'artifacts/ui', which = 'lobby,fight,result'] = process.argv.slice(2);
const wanted = new Set(which.split(','));
// Telegram fullscreen: the system bar (safeAreaInset) and Telegram's own
// close / menu buttons (contentSafeAreaInset) in the top corners.
const screens = [
  { name: 'portrait', viewport: { width: 390, height: 844 }, safe: { top: 47, bottom: 34, left: 0, right: 0 }, content: { top: 46, bottom: 0, left: 0, right: 0 } },
  { name: 'landscape', viewport: { width: 844, height: 390 }, safe: { top: 0, bottom: 21, left: 47, right: 47 }, content: { top: 46, bottom: 0, left: 0, right: 0 } },
];
const stub = s => `window.Telegram={WebApp:{initData:'',initDataUnsafe:{},version:'8.0',platform:'ios',isFullscreen:true,
  safeAreaInset:${JSON.stringify(s.safe)},contentSafeAreaInset:${JSON.stringify(s.content)},viewportStableHeight:${s.viewport.height},
  isVersionAtLeast:()=>true,ready(){},expand(){},disableVerticalSwipes(){},setHeaderColor(){},setBackgroundColor(){},onEvent(){},
  requestFullscreen(){},exitFullscreen(){},openTelegramLink(){},HapticFeedback:{impactOccurred(){},notificationOccurred(){}},
  BackButton:{show(){},hide(){},onClick(){}}}};`;
// Telegram's floating header buttons, drawn over the shot for reference.
const chrome = `(() => { for (const [side, label] of [['left', '✕ Закрыть'], ['right', '⋯']]) {
  const b = document.createElement('div'); b.textContent = label;
  b.style.cssText = 'position:fixed;z-index:99;top:' + (window.__safeTop + 6) + 'px;' + side + ':' + (8 + window.__safeSide) + 'px;height:32px;padding:0 14px;border-radius:16px;background:#ffffff30;color:#fff;font:600 14px sans-serif;display:flex;align-items:center;pointer-events:none';
  document.body.append(b); } })()`;

const app = await createArena({ dev: true });
app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
const base = `http://127.0.0.1:${app.server.address().port}`;
const browser = await chromium.launch({ headless: true, args: ['--enable-unsafe-swiftshader'] });
const errors = [];
await mkdir(out, { recursive: true });
const rendered = p => p.evaluate(() => JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)));
try {
  for (const s of screens) {
    const context = await browser.newContext({ viewport: s.viewport, hasTouch: true, isMobile: true, deviceScaleFactor: 2 });
    await context.route('https://telegram.org/**', r => r.fulfill({ contentType: 'text/javascript', body: stub(s) }));
    const page = await context.newPage(); page.on('pageerror', e => errors.push(e.message));
    await page.goto(base); await page.waitForSelector('#loader', { state: 'hidden', timeout: 60000 });
    await page.evaluate(([top, side]) => { window.__safeTop = top; window.__safeSide = side; }, [s.safe.top, s.safe.left]);
    await page.evaluate(chrome);
    const shot = name => page.screenshot({ path: `${out}/${s.name}-${name}.png` });
    if (wanted.has('lobby')) { await page.waitForTimeout(400); await shot('lobby'); }
    if (!wanted.has('fight') && !wanted.has('result')) { await context.close(); continue; }
    await page.click('#practice'); await page.waitForTimeout(3200);
    if (wanted.has('fight')) await shot('fight');
    if (wanted.has('result')) {
      // Dummy bot; the left fighter wins both rounds with repeated strikes.
      await page.click('#training-mode');
      for (let i = 0; i < 900 && (await rendered(page)).phase !== 3; i++) {
        const st = await rendered(page);
        const [me, bot] = st.fighters;
        const key = Math.abs(bot.x - me.x) > 1000 ? 'd' : ['j', 'k', 'u'][i % 3];
        await page.keyboard.down(key); await page.waitForTimeout(key === 'd' ? 120 : 40); await page.keyboard.up(key); await page.waitForTimeout(60);
      }
      let at = 0;
      for (const t of [1200, 4000, 9000]) { await page.waitForTimeout(t - at); at = t; await shot('result-' + t); }
    }
    await context.close();
  }
} finally {
  await browser.close(); app.server.close();
}
if (errors.length) { console.error(errors.join('\n')); process.exitCode = 1; }
console.log('UI shots in', out);
