// Plays every sound effect and music track in a headless browser and prints
// their output levels (peak and RMS of the mix): nothing should be silent,
// nothing should clip, and the music should sit well under the hits.
//   node scripts/sound-check.mjs
import { resolve } from 'node:path';
import { createArena } from '../server/index.mjs';
process.env.PLAYWRIGHT_BROWSERS_PATH ||= resolve('.browsers');
const { chromium } = await import('playwright');

const app = await createArena({ dev: true });
app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
const browser = await chromium.launch({ headless: true, args: ['--enable-unsafe-swiftshader', '--autoplay-policy=no-user-gesture-required'] });
const page = await browser.newPage();
const errors = []; page.on('pageerror', e => errors.push(e.message));
await page.route('https://telegram.org/**', r => r.fulfill({ contentType: 'text/javascript', body: '' }));
let failed = false;
try {
  await page.goto(`http://127.0.0.1:${app.server.address().port}`);
  await page.waitForSelector('#loader', { state: 'hidden', timeout: 60000 });
  await page.mouse.click(5, 5);
  const measure = (kind, name, args, seconds, heat) => page.evaluate(({ kind, name, args, seconds, heat }) =>
    window.arenaTest.sound.measure(kind, name, { args, heat }, seconds), { kind, name, args, seconds, heat });
  const effects = [['blow light', 'blow', [0.3, 1]], ['blow heavy', 'blow', [0.9, 1]], ['whoosh', 'whoosh', [0.5]], ['block', 'block', []],
    ['parry', 'parry', []], ['guard break', 'shatterGuard', []], ['grab', 'grab', []], ['throw slam', 'slam', []], ['crash', 'crash', [false]],
    ['glass', 'crash', [true]], ['knockout', 'knockout', []], ['beep', 'beep', [false]], ['bell', 'bell', [2]], ['win jingle', 'jingle', [true]],
    ['lose jingle', 'jingle', [false]], ['click', 'click', []]];
  console.log('effect          peak   rms');
  for (const [label, name, args] of effects) {
    const m = await measure('effect', name, args, 2);
    console.log(label.padEnd(14), m.peak.toFixed(3), m.rms.toFixed(3));
    if (m.peak < 0.015 || m.peak > 0.99) failed = true;
  }
  console.log('music           peak   rms');
  for (const [track, heat] of [['lobby', 0], ['arcade', 0], ['arcade', 1], ['brawl', 0], ['brawl', 1]]) {
    const m = await measure('music', track, [], 12, heat);
    console.log((track + (heat ? ' hot' : '')).padEnd(14), m.peak.toFixed(3), m.rms.toFixed(3));
    if (m.rms < 0.01 || m.peak > 0.99) failed = true;
  }
} finally { await browser.close(); app.server.close(); }
if (errors.length) { console.error(errors.join('\n')); failed = true; }
if (failed) { console.error('Sound check failed'); process.exitCode = 1; } else console.log('Sound check passed');
process.exit(process.exitCode ?? 0);
