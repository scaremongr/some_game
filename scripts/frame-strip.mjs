// Records a training sequence frame by frame on a manual clock (the page's
// performance.now and requestAnimationFrame are driven by this script), so
// motion can be reviewed at a true 60 Hz however slowly the page renders.
//   node scripts/frame-strip.mjs <out-dir> [script]
// script: keys and durations, e.g. "d:1500,_:400,a:1500" (key or '_' for
// nothing, ms of virtual time); every 4th frame (15 fps) is saved as PNG,
// FIGHTER=<id> picks the fighter. A third argument edits the training state
// first (see below), e.g. to stage a knockout.
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createArena } from '../server/index.mjs';
process.env.PLAYWRIGHT_BROWSERS_PATH ||= resolve('.browsers');
const { chromium } = await import('playwright');
const EVERY = Number(process.env.EVERY || 4);
const [outDir = 'artifacts/strip', script = 'd:1500,_:500,a:1500,_:500', setup = ''] = process.argv.slice(2);
await mkdir(outDir, { recursive: true });
const app = await createArena({ dev: true });
app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
const browser = await chromium.launch({ headless: true, args: ['--enable-unsafe-swiftshader'] });
const context = await browser.newContext({ viewport: { width: 960, height: 540 } });
if (process.env.FIGHTER) await context.addInitScript(id => { try { localStorage.setItem('pulse-fighter', id); } catch {} }, process.env.FIGHTER);
// The manual clock: off until the fight starts, then every frame is one call.
await context.addInitScript(() => {
  const realNow = performance.now.bind(performance), realRaf = window.requestAnimationFrame.bind(window);
  const clock = window.__clock = { manual: false, now: 0, queue: [] };
  performance.now = () => clock.manual ? clock.now : realNow();
  // gl.js times frames with Date.now: it follows the same clock.
  const realDate = Date.now.bind(Date);
  Date.now = () => clock.manual ? clock.date + (clock.now - clock.base) : realDate();
  window.requestAnimationFrame = cb => {
    if (clock.manual) { clock.queue.push(cb); return 0; }
    return realRaf(t => clock.manual ? clock.queue.push(cb) : cb(t));
  };
  window.__start = () => { clock.now = clock.base = realNow(); clock.date = realDate(); clock.manual = true; };
  window.__frame = ms => { clock.now += ms; const q = clock.queue; clock.queue = []; for (const cb of q) cb(clock.now); };
});
const p = await context.newPage();
await p.route('https://telegram.org/**', r => r.fulfill({ contentType: 'text/javascript', body: '' }));
try {
  await p.goto(`http://127.0.0.1:${app.server.address().port}`);
  await p.waitForSelector('#loader', { state: 'hidden', timeout: 60000 });
  await p.click('#practice'); await p.click('#training-mode');
  await p.waitForFunction(() => JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)).phase === 1, {}, { timeout: 10000 });
  await p.waitForTimeout(500);
  // Optional setup: a JS body run on the training state `s`, e.g.
  // "s.fighters[1].hp=5;s.fighters[0].x=-450;s.fighters[1].x=450" (dev only).
  if (setup) await p.evaluate(code => window.arenaTrainingEdit(new Function('s', code)), setup);
  await p.evaluate(() => window.__start());
  await p.waitForTimeout(200);
  const advance = ms => p.evaluate(ms => window.__frame(ms), ms);
  const codes = { d: 'KeyD', a: 'KeyA', s: 'KeyS', c: 'KeyC', j: 'KeyJ', k: 'KeyK', u: 'KeyU', l: 'KeyL', w: 'KeyW' };
  let frame = 0;
  for (const part of script.split(',')) {
    const [keys, ms] = part.split(':');
    const down = keys === '_' ? [] : keys.split('+');
    for (const k of down) await p.evaluate(code => window.dispatchEvent(new KeyboardEvent('keydown', { code })), codes[k]);
    for (let t = 0; t < Number(ms); t += 1000 / 60) {
      await advance(1000 / 60);
      if (frame++ % EVERY === 0) await p.screenshot({ path: `${outDir}/f-${String(frame).padStart(4, '0')}.png` });
    }
    for (const k of down) await p.evaluate(code => window.dispatchEvent(new KeyboardEvent('keyup', { code })), codes[k]);
  }
  console.log(`${Math.ceil(frame / EVERY)} frames in ${outDir}`);
} finally { await browser.close(); await app.close(); }
