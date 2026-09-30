// Renders the menu portrait of every fighter in assets/fighters/roster.json.
//   node scripts/fighter-portraits.mjs [id ...]      (after build-web.ps1)
// Each fighter stands in its fight stance in the arena, seen three-quarters
// from the front; the picture is saved as assets/fighters/<id>.jpg.
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createArena } from '../server/index.mjs';
process.env.PLAYWRIGHT_BROWSERS_PATH ||= resolve('.browsers');
const { chromium } = await import('playwright');

const only = process.argv.slice(2);
const roster = JSON.parse(await readFile('assets/fighters/roster.json', 'utf8')).filter(f => !only.length || only.includes(f.id));
const app = await createArena({ dev: true });
app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
const browser = await chromium.launch({ headless: true, args: ['--enable-unsafe-swiftshader'] });
const page = await browser.newPage({ viewport: { width: 240, height: 320 } });
await page.route('https://telegram.org/**', r => r.fulfill({ contentType: 'text/javascript', body: '' }));
try {
  await page.goto(`http://127.0.0.1:${app.server.address().port}`);
  await page.waitForSelector('#loader', { state: 'hidden', timeout: 60000 });
  await page.addStyleTag({ content: 'body>*:not(#glcanvas){display:none!important}' });
  for (const fighter of roster) {
    await page.evaluate(fighter => {
      window.arenaFighters = [fighter, fighter];
      const base = window.__baseState ||= JSON.parse(new TextDecoder().decode(window.arenaRenderBytes));
      const state = structuredClone(base);
      state.phase = 1; state.freeze = 0;
      state.fighters.forEach((f, side) => Object.assign(f, { x: side ? 2600 : -560, y: 0, action: 0, frame: 0, guard: false, crouch: false, previous: 0 }));
      // Three-quarters from the front of the left fighter, chest up.
      window.arenaDebugCamera = [0.25, 1.46, 1.3, -0.56, 1.24, 0, 0.6, 1];
      window.arenaRenderBytes = new TextEncoder().encode(JSON.stringify(state));
    }, fighter);
    // The renderer reports readiness when it changes: give it a frame to see
    // the new request, then wait for both sides.
    await page.waitForTimeout(200);
    await page.waitForFunction(() => window.arenaAvatarsReady === 3, {}, { timeout: 30000 });
    await page.waitForTimeout(400);
    const file = `assets/fighters/${fighter.id}.jpg`;
    await page.screenshot({ path: file, type: 'jpeg', quality: 82 });
    console.log(`Portrait: ${file}`);
  }
} finally { await browser.close(); await app.close(); }
