// Online prediction under a simulated round trip: a press must show on the
// presser's screen within a couple of frames, and the predicted fight must
// agree with the server once both sides stop.
//   node scripts/netcode-test.mjs [rtt-ms]      (default 120)
import assert from 'node:assert/strict';
import { access } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createArena } from '../server/index.mjs';
try { await access('.browsers'); process.env.PLAYWRIGHT_BROWSERS_PATH ||= resolve('.browsers'); } catch {}
const { chromium } = await import('playwright');
const rtt = Number(process.argv[2] ?? 120);
const app = await createArena({ dev: true, latency: rtt });
app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
const base = `http://127.0.0.1:${app.server.address().port}`;
const browser = await chromium.launch({ headless: true, args: ['--enable-unsafe-swiftshader'] });
const errors = [];
async function page(query) {
  const context = await browser.newContext({ viewport: { width: Number(process.env.NET_VIEW || 320), height: Number(process.env.NET_VIEW || 320) * 3 / 4 } });
  await context.route('https://telegram.org/**', route => route.fulfill({ contentType: 'text/javascript', body: '' }));
  const p = await context.newPage(); p.on('pageerror', e => errors.push(e.message));
  await p.goto(base + query); await p.waitForSelector('#loader', { state: 'hidden', timeout: 30000 }); return p;
}
const rendered = p => p.evaluate(() => JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)));
// Milliseconds from a key press until the page's rendered state shows the
// presser attacking (the page records the time itself, no test round trips).
async function pressToScreen(p, side) {
  return p.evaluate(side => new Promise(done => {
    const start = performance.now();
    window.dispatchEvent(new KeyboardEvent('keydown', { code: 'KeyJ' }));
    setTimeout(() => window.dispatchEvent(new KeyboardEvent('keyup', { code: 'KeyJ' })), 50);
    const check = () => {
      const s = JSON.parse(new TextDecoder().decode(window.arenaRenderBytes));
      if ([1, 11, 17].includes(s.fighters[side].action)) done(performance.now() - start);
      else if (performance.now() - start > 2000) done(Infinity);
      else requestAnimationFrame(check);
    };
    check();
  }), side);
}
async function match(query) {
  const a = await page(query), b = await page(query);
  await a.click('#quick'); await b.click('#quick');
  await a.waitForSelector('#hud:not(.hidden)'); await b.waitForSelector('#hud:not(.hidden)');
  await a.waitForFunction(() => JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)).phase === 1, {}, { timeout: 10000 });
  await a.waitForTimeout(600);
  const side = await a.evaluate(() => document.getElementById('mode').textContent.includes('СЛЕВА') ? 0 : 1);
  const frame = await a.evaluate(() => new Promise(done => { let n = 0; const t0 = performance.now(); const tick = () => (++n < 30 ? requestAnimationFrame(tick) : done((performance.now() - t0) / n)); requestAnimationFrame(tick); }));
  const times = [];
  for (let i = 0; i < 6; i++) { times.push(await pressToScreen(a, side)); await a.waitForTimeout(700); }
  // Walk in and trade blows for a while, then stop and compare with the server.
  const toward = side === 0 ? 'KeyD' : 'KeyA';
  await a.keyboard.down(toward === 'KeyD' ? 'd' : 'a'); await a.waitForTimeout(900); await a.keyboard.up(toward === 'KeyD' ? 'd' : 'a');
  for (let i = 0; i < 8; i++) {
    await a.keyboard.press(['j', 'u', 'k', 'j'][i % 4]); await b.keyboard.press(['s', 'j', 'u', 'l'][i % 4]); await a.waitForTimeout(260);
  }
  await a.waitForTimeout(1500);
  const [mine, theirs] = [await rendered(a), await rendered(b)];
  const stats = await a.evaluate(() => window.arenaNetStats);
  await a.context().close(); await b.context().close();
  times.sort((x, y) => x - y);
  return { stats, frame, median: times[3], worst: times[5], hp: [mine.fighters.map(f => f.hp), theirs.fighters.map(f => f.hp)] };
}
try {
  const predicted = await match('');
  const delayed = await match('?predict=0');
  console.log(`RTT ${rtt} ms. Press to screen: predicted median ${predicted.median.toFixed(0)} ms (worst ${predicted.worst.toFixed(0)}), without prediction ${delayed.median.toFixed(0)} ms (worst ${delayed.worst.toFixed(0)}).`);
  console.log(`Frame time in this headless browser: ${predicted.frame.toFixed(0)} ms`);
  const st = predicted.stats;
  console.log(`Snapshots ${st.snapshots}, corrected predictions ${st.corrections} (${(st.corrections / st.snapshots * 100).toFixed(0)}%), lead ${st.lead.toFixed(1)} ticks, clock error ${(st.error ?? 0).toFixed(1)} ticks`);
  console.log(`After the exchange both screens agree: ${JSON.stringify(predicted.hp)}`);
  // Headless WebGL renders slowly: count in frames of this browser.
  assert.ok(predicted.median < predicted.frame * 2 + 20, 'a press shows on the next frame or so');
  assert.ok(delayed.median > predicted.median + rtt * 0.6, 'prediction removes the round trip');
  assert.deepEqual(predicted.hp[0], predicted.hp[1], 'both clients settle on the server state');
  assert.deepEqual(errors, []);
  console.log('Netcode PASS');
} finally { await browser.close(); await app.close(); }
