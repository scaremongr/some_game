// Records a scripted training bout to video for motion review.
//   node scripts/fight-video.mjs [out-dir] [scenario]
// Scenarios: moves (default) — walking, strings, jumps, specials against the dummy;
// air — throws, jumps in every direction, air kick, blocking; ground — crouch,
// crouch walk and guard, room smash; defense — blocks, lows, overhead and the
// low kick -> uppercut against the guard dummy; spar; ko. FIGHTER=<id> picks the fighter.
import { mkdir, readdir, rename } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { createArena } from '../server/index.mjs';
process.env.PLAYWRIGHT_BROWSERS_PATH ||= resolve('.browsers');
const { chromium } = await import('playwright');
const [outDir = 'artifacts/video', scenario = 'moves', w = '1280', h = '720'] = process.argv.slice(2);
await mkdir(outDir, { recursive: true });
const app = await createArena({ dev: true });
app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
const browser = await chromium.launch({ headless: true, args: ['--enable-unsafe-swiftshader'] });
const size = { width: +w, height: +h };
const context = await browser.newContext({ viewport: size, hasTouch: size.width < 600, recordVideo: { dir: outDir, size } });
// FIGHTER=<id> picks the player's fighter (the bot gets a random other one).
if (process.env.FIGHTER) await context.addInitScript(id => { try { localStorage.setItem('pulse-fighter', id); } catch {} }, process.env.FIGHTER);
const p = await context.newPage();
await p.route('https://telegram.org/**', r => r.fulfill({ contentType: 'text/javascript', body: '' }));
const state = () => p.evaluate(() => JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)));
const tap = async (key, ms = 60) => { await p.keyboard.down(key); await p.waitForTimeout(ms); await p.keyboard.up(key); };
const hold = async (keys, ms) => { for (const k of keys) await p.keyboard.down(k); await p.waitForTimeout(ms); for (const k of keys) await p.keyboard.up(k); };
try {
  await p.goto(`http://127.0.0.1:${app.server.address().port}`);
  await p.waitForSelector('#loader', { state: 'hidden', timeout: 60000 });
  await p.click('#practice'); await p.click('#training-mode');
  await p.waitForFunction(() => JSON.parse(new TextDecoder().decode(window.arenaRenderBytes)).phase === 1, {}, { timeout: 10000 });
  if (scenario === 'moves') {
    await p.waitForTimeout(400);
    await hold(['d'], 700); await p.waitForTimeout(300);
    await hold(['a'], 400); await p.waitForTimeout(300);
    await hold(['d'], 500);
    await tap('j'); await p.waitForTimeout(140); await tap('j'); await p.waitForTimeout(160); await tap('u'); await p.waitForTimeout(900);
    await hold(['d'], 500); await tap('k'); await p.waitForTimeout(1200);
    await hold(['d'], 500); await hold(['c', 'u'], 120); await p.waitForTimeout(1300);
    await hold(['d'], 400); await hold(['c', 'k'], 120); await p.waitForTimeout(1300);
    await hold(['w', 'd'], 200); await p.waitForTimeout(250); await tap('u'); await p.waitForTimeout(900);
    await hold(['d'], 400); await tap('i'); await p.waitForTimeout(1200);
    await tap('l'); await p.waitForTimeout(1200);
    await hold(['s'], 600); await hold(['c'], 600);
    await tap('Space'); await p.waitForTimeout(700);
  } else if (scenario === 'air') {
    // Throws, jumps in every direction, the air kick, then blocking a sparring bot.
    const jump = async (dir, ms = 120) => { if (dir) await p.keyboard.down(dir); await p.waitForTimeout(70); await p.keyboard.down('w'); await p.waitForTimeout(ms); await p.keyboard.up('w'); if (dir) await p.keyboard.up(dir); };
    await p.waitForTimeout(300);
    await hold(['d'], 600); await tap('l'); await p.waitForTimeout(1800);
    await hold(['d'], 500); await tap('l'); await p.waitForTimeout(1800);
    await hold(['a'], 500); await jump('d'); await p.waitForTimeout(260); await tap('u'); await p.waitForTimeout(1000);
    await jump('a'); await p.waitForTimeout(1000);
    await jump(null); await p.waitForTimeout(1000);
    await jump('d'); await p.waitForTimeout(1000);
    await p.click('#training-mode'); await p.click('#training-mode'); await p.click('#training-mode');
    await hold(['s'], 4000);
  } else if (scenario === 'ground') {
    // Crouch, crouch walking both ways, crouch guard, then the room smash.
    await p.waitForTimeout(300);
    await hold(['c'], 900);
    await hold(['c', 'd'], 900); await hold(['c', 'a'], 900);
    await hold(['c', 's'], 900); await p.waitForTimeout(300);
    await tap('q'); await p.waitForTimeout(1200);
    await hold(['d'], 500); await tap('q'); await p.waitForTimeout(1400);
  } else if (scenario === 'defense') {
    // Guard dummy: strings into the standing guard, a low kick under it; the
    // crouching guard stops the low but not the overhead; then the low kick ->
    // uppercut and holding block against the sparring bot. Blocked blows push
    // the dummy back, so each attack starts with a step in.
    const step = () => hold(['d'], 260);
    await p.click('#training-mode');
    await hold(['d'], 600);
    await tap('j'); await p.waitForTimeout(120); await tap('j'); await p.waitForTimeout(700);
    await step(); await tap('u'); await p.waitForTimeout(800);
    await step(); await hold(['c', 'j'], 120); await p.waitForTimeout(800);
    await p.click('#training-mode');
    await step(); await hold(['c', 'j'], 120); await p.waitForTimeout(800);
    await step(); await hold(['c', 'u'], 120); await p.waitForTimeout(1000);
    await step(); await tap('k'); await p.waitForTimeout(1500);
    await p.click('#training-mode'); await p.click('#training-mode');
    await step(); await hold(['c', 'j'], 120); await p.waitForTimeout(220); await hold(['c', 'k'], 120); await p.waitForTimeout(1800);
    await p.click('#training-mode'); await p.click('#training-mode'); await p.click('#training-mode');
    await hold(['s'], 4500);
  } else if (scenario === 'spar') {
    await p.click('#training-mode'); await p.click('#training-mode'); await p.click('#training-mode');
    for (let i = 0; i < 14; i++) { await hold(['d'], 350); await tap(['j', 'k', 'u', 'l'][i % 4]); await p.waitForTimeout(600); }
  } else if (scenario === 'ko') {
    for (let i = 0; i < 16 && (await state()).fighters[1].hp > 0; i++) {
      await hold(['d'], 300); await tap('j'); await p.waitForTimeout(150); await tap('j'); await p.waitForTimeout(160); await tap('u'); await p.waitForTimeout(700);
    }
    await p.waitForTimeout(2600);
  }
  console.log('final', JSON.stringify((await state()).fighters.map(f => f.hp)));
} finally {
  const video = p.video(); await context.close();
  const path = await video.path(); await rename(path, join(outDir, `${scenario}.webm`));
  await browser.close(); await app.close();
  console.log(`Video: ${join(outDir, scenario + '.webm')}`);
}
