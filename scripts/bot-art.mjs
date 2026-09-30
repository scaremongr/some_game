// Renders the Telegram bot's pictures from the real game (after build-web.ps1):
//   node scripts/bot-art.mjs
// assets/bot/cover.jpg     1280x720  — the /start message picture (shipped)
// assets/bot/card-bg.jpg   1200x630  — background of victory cards (shipped)
// docs/bot-kit/avatar.jpg  640x640   — bot profile photo (BotFather /setuserpic)
// docs/bot-kit/description.jpg 640x360 — «What can this bot do?» picture
// docs/bot-kit/preview-*.jpg 720x1280 — Main Mini App preview screenshots
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createArena } from '../server/index.mjs';
process.env.PLAYWRIGHT_BROWSERS_PATH ||= resolve('.browsers');
const { chromium } = await import('playwright');

const out = { cover: 'assets/bot/cover.jpg', kit: 'docs/bot-kit' };
await mkdir('assets/bot', { recursive: true }); await mkdir(out.kit, { recursive: true }); await mkdir('artifacts/bot-art', { recursive: true });
const app = await createArena({ dev: true });
app.server.listen(0, '127.0.0.1'); await new Promise(r => app.server.once('listening', r));
const base = `http://127.0.0.1:${app.server.address().port}`;
const browser = await chromium.launch({ headless: true, args: ['--enable-unsafe-swiftshader'] });
const fighter = id => id === 'medea' ? { model: 'assets/character.glb', pack: 'assets/fight.pack' } : { model: `assets/fighters/${id}.glb`, pack: `assets/fighters/${id}.pack` };

// A clean frame of the arena: no interface, chosen fighters, state and camera.
async function scene(file, { size, fighters, f0 = {}, f1 = {}, x = [-560, 560], camera, wait = 500 }) {
  const page = await browser.newPage({ viewport: size, deviceScaleFactor: 2 });
  await page.route('https://telegram.org/**', r => r.fulfill({ contentType: 'text/javascript', body: '' }));
  await page.goto(base); await page.waitForSelector('#loader', { state: 'hidden', timeout: 90000 });
  await page.addStyleTag({ content: 'body>*:not(#glcanvas),.vignette{display:none!important}' });
  await page.evaluate(({ fighters, f0, f1, x, camera }) => {
    window.arenaFighters = fighters;
    const state = JSON.parse(new TextDecoder().decode(window.arenaRenderBytes));
    state.phase = 1; state.freeze = 0;
    state.fighters.forEach((f, side) => Object.assign(f, { x: x[side], y: 0, action: 0, frame: 0, guard: false, crouch: false, previous: 0 }, side ? f1 : f0));
    window.arenaDebugCamera = camera;
    window.arenaRenderBytes = new TextEncoder().encode(JSON.stringify(state));
  }, { fighters, f0, f1, x, camera });
  await page.waitForTimeout(300);
  await page.waitForFunction(() => window.arenaAvatarsReady === 3, {}, { timeout: 60000 });
  await page.waitForTimeout(wait);
  await page.screenshot({ path: file });
  await page.close();
}

// Lays the logo over a frame and saves a JPEG of exactly `w` x `h`.
async function compose(frame, target, w, h, layout) {
  const img = 'data:image/png;base64,' + (await readFile(frame)).toString('base64');
  const html = `<!doctype html><html><head><style>
    body{margin:0;width:${w}px;height:${h}px;overflow:hidden;font-family:Inter,"Segoe UI",Arial,sans-serif;background:#080e18}
    .bg{position:absolute;inset:0;background:url(${img}) center/cover}
    .shade{position:absolute;inset:0;background:${layout.shade}}
    .logo{position:absolute;${layout.logo};color:#fff;font-weight:900;letter-spacing:.06em;line-height:1;text-shadow:0 4px 24px #0009}
    .logo b{color:#57f0ce;margin-right:.18em}
    .logo.badge b{margin:0;text-shadow:0 0 28px #57f0ce99,0 6px 20px #000c}
    .tag{position:absolute;${layout.tag};color:#e9fbff;font-weight:700;letter-spacing:.02em;text-shadow:0 2px 12px #000a}
    .tag em{font-style:normal;color:#57f0ce}
  </style></head><body><div class=bg></div><div class=shade></div>
  <div class="logo${layout.badge ? ' badge' : ''}"><b>ϟ</b>${layout.badge ? '' : layout.title ?? 'PULSE'}</div>${layout.text ? `<div class=tag>${layout.text}</div>` : ''}</body></html>`;
  const page = await browser.newPage({ viewport: { width: w, height: h } });
  await page.setContent(html); await page.waitForTimeout(300);
  await page.screenshot({ path: target, type: 'jpeg', quality: 88 });
  await page.close();
  console.log('Picture:', target);
}

// Real interface screenshots for the Mini App preview (phone, portrait).
async function preview(file, setup) {
  const ctx = await browser.newContext({ viewport: { width: 360, height: 640 }, deviceScaleFactor: 2, isMobile: true, hasTouch: true });
  await ctx.addInitScript(() => { try { localStorage.setItem('pulse-fighter', 'kachujin'); } catch {} });
  const page = await ctx.newPage();
  await page.route('https://telegram.org/**', r => r.fulfill({ contentType: 'text/javascript', body: '' }));
  await page.goto(base); await page.waitForSelector('#loader', { state: 'hidden', timeout: 90000 });
  await page.evaluate(() => { const c = document.getElementById('connection'); if (c) c.textContent = 'TELEGRAM FIGHT CLUB'; });
  await setup(page);
  await page.screenshot({ path: file, type: 'jpeg', quality: 88 });
  await ctx.close();
  console.log('Preview:', file);
}

try {
  const fight = [fighter('kachujin'), fighter('vanguard')];
  // Cover: a spinning back kick landing, low wide camera.
  await scene('artifacts/bot-art/cover.png', {
    size: { width: 640, height: 360 }, fighters: fight, x: [-520, 520],
    f0: { action: 2, frame: 22 }, f1: { action: 5, stun: 24, frame: 3 },
    camera: [0.35, 0.95, 3.2, 0.1, 1.0, 0, 0.62, 1],
  });
  // The same frame without lettering: the background of victory cards.
  await scene('assets/bot/card-bg.jpg', {
    size: { width: 600, height: 315 }, fighters: fight, x: [-520, 520],
    f0: { action: 2, frame: 22 }, f1: { action: 5, stun: 24, frame: 3 },
    camera: [0.35, 0.95, 3.2, 0.1, 1.0, 0, 0.62, 1],
  });
  console.log('Picture: assets/bot/card-bg.jpg');
  await compose('artifacts/bot-art/cover.png', out.cover, 1280, 720, {
    shade: 'linear-gradient(90deg,#080e18d0 0%,#080e1860 38%,#0000 60%)',
    logo: 'left:64px;top:60px;font-size:96px', tag: 'left:70px;top:176px;font-size:34px;max-width:520px;line-height:1.35',
    text: 'Файтинг 1 на 1<br><em>прямо в Telegram</em>',
  });
  await compose('artifacts/bot-art/cover.png', `${out.kit}/description.jpg`, 640, 360, {
    shade: 'linear-gradient(90deg,#080e18d0 0%,#080e1860 40%,#0000 62%)',
    logo: 'left:32px;top:30px;font-size:48px', tag: 'left:35px;top:90px;font-size:18px;line-height:1.35',
    text: 'Файтинг 1 на 1<br><em>прямо в Telegram</em>',
  });
  // Avatar: a fighter in guard, face centred for the round crop.
  await scene('artifacts/bot-art/avatar.png', {
    size: { width: 400, height: 400 }, fighters: [fighter('kachujin'), fighter('ninja')], x: [-560, 2600],
    f0: { guard: true }, camera: [0.05, 1.42, 1.25, -0.56, 1.3, 0, 0.62, 1],
  });
  await compose('artifacts/bot-art/avatar.png', `${out.kit}/avatar.jpg`, 640, 640, {
    shade: 'radial-gradient(circle at 46% 40%,#0000 34%,#080e18c8 70%)',
    logo: 'right:118px;bottom:92px;font-size:170px', badge: true,
  });
  // Mini App previews: the fighter picker, a fight, a throw.
  await preview(`${out.kit}/preview-1.jpg`, async page => { await page.waitForTimeout(800); });
  await preview(`${out.kit}/preview-2.jpg`, async page => {
    await page.click('#practice'); await page.waitForTimeout(4200);
    await page.evaluate(() => {
      window.arenaDebugCamera = null;
    });
    await page.waitForTimeout(1500);
  });
  await preview(`${out.kit}/preview-3.jpg`, async page => {
    await page.click('#practice'); await page.waitForTimeout(4200);
    // Walk in and grab.
    await page.keyboard.down('d'); await page.waitForTimeout(700); await page.keyboard.up('d');
    await page.keyboard.press('l'); await page.waitForTimeout(420);
  });
} finally { await browser.close(); await app.close(); }
